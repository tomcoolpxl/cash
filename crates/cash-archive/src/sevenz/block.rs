/// Represents a compression block.
///
/// A block contains one or more coders (compression/filter methods) that are chained
/// together to process data.
#[derive(Debug, Default, Clone)]
pub struct Block {
    /// Coders (compression/filter methods) in this block.
    pub coders: Vec<Coder>,
    /// Whether this block has a CRC checksum.
    pub has_crc: bool,
    /// CRC32 checksum of the block data.
    pub crc: u64,
    pub(crate) total_input_streams: usize,
    pub(crate) total_output_streams: usize,
    pub(crate) bind_pairs: Vec<BindPair>,
    pub(crate) packed_streams: Vec<u64>,
    pub(crate) unpack_sizes: Vec<u64>,
    pub(crate) num_unpack_sub_streams: usize,
}

impl Block {
    /// Whether one of the block's coders is 7-Zip's AES.
    pub fn is_encrypted(&self) -> bool {
        self.coders.iter().any(|coder| {
            coder.encoder_method_id() == crate::sevenz::EncoderMethod::ID_AES256_SHA256
        })
    }

    /// The streams the block's coders are bound by, as (input, output) stream indexes.
    pub fn bind_pairs(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.bind_pairs.iter().map(|bp| (bp.in_index, bp.out_index))
    }

    /// The input streams that are packed streams, in the order they are stored.
    pub fn packed_streams(&self) -> &[u64] {
        &self.packed_streams
    }

    /// Each coder's output size, by output stream.
    pub fn unpack_sizes(&self) -> &[u64] {
        &self.unpack_sizes
    }

    /// How many entries' data the block holds.
    pub const fn sub_stream_count(&self) -> usize {
        self.num_unpack_sub_streams
    }

    /// The coder that gives out the block's data, the last the data passes through
    /// when read: its filter, when it has one (7-Zip's `UnpackCoder`).
    pub fn unpack_coder(&self) -> Option<&Coder> {
        let mut first_out = 0u64;
        for coder in &self.coders {
            let outs = first_out..first_out + coder.num_out_streams;
            if outs
                .clone()
                .any(|out| self.find_bind_pair_for_out_stream(out).is_none())
            {
                return Some(coder);
            }
            first_out = outs.end;
        }
        None
    }

    pub(crate) fn find_bind_pair_for_in_stream(&self, index: u64) -> Option<&BindPair> {
        self.bind_pairs.iter().find(|bp| bp.in_index == index)
    }

    pub(crate) fn find_bind_pair_for_out_stream(&self, index: u64) -> Option<&BindPair> {
        self.bind_pairs.iter().find(|bp| bp.out_index == index)
    }

    /// Returns the total uncompressed size of data in this block.
    pub fn get_unpack_size(&self) -> u64 {
        if self.total_output_streams == 0 {
            return 0;
        }
        for i in (0..self.total_output_streams).rev() {
            if self.find_bind_pair_for_out_stream(i as u64).is_none() {
                return self.unpack_sizes[i];
            }
        }
        0
    }

    /// Returns the uncompressed size for a specific coder within this block.
    ///
    /// # Arguments
    /// * `coder` - The coder to get the unpack size for
    pub fn get_unpack_size_for_coder(&self, coder: &Coder) -> u64 {
        for i in 0..self.coders.len() {
            if std::ptr::eq(&raw const self.coders[i], coder) {
                return self.unpack_sizes[i];
            }
        }
        0
    }

    /// Returns the uncompressed size for the coder at the specified index.
    ///
    /// # Arguments
    /// * `index` - The index of the coder to get the unpack size for
    pub fn get_unpack_size_at_index(&self, index: usize) -> u64 {
        self.unpack_sizes.get(index).copied().unwrap_or_default()
    }

    /// Returns an iterator over the coders in their processing order.
    ///
    /// Coders are chained together in blocks, and this iterator follows the chain
    /// from the first coder to the last in their proper execution order.
    pub fn ordered_coder_iter(&self) -> OrderedCoderIter<'_> {
        OrderedCoderIter::new(self)
    }
}

/// Represents a single coder within a compression block.
///
/// A coder defines a specific compression method, filter, or encryption method
/// used to process data within a block.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Coder {
    encoder_method_id: [u8; 0xF],
    pub(crate) id_size: usize,
    pub(crate) num_in_streams: u64,
    pub(crate) num_out_streams: u64,
    pub(crate) properties: Vec<u8>,
}

impl Coder {
    /// Returns the encoder method ID for this coder.
    ///
    /// This ID identifies the specific compression method, filter, or encryption
    /// method used by this coder.
    pub fn encoder_method_id(&self) -> &[u8] {
        &self.encoder_method_id[0..self.id_size]
    }

    /// Returns this coder's properties: the method-specific parameters stored alongside the method
    /// ID, empty for a method that takes none. How to interpret them depends on
    /// [`encoder_method_id`](Self::encoder_method_id).
    pub fn properties(&self) -> &[u8] {
        &self.properties
    }

    /// How many streams the coder takes in and gives out (BCJ2 takes four).
    pub const fn stream_counts(&self) -> (u64, u64) {
        (self.num_in_streams, self.num_out_streams)
    }

    pub(crate) fn decompression_method_id_mut(&mut self) -> &mut [u8] {
        &mut self.encoder_method_id[0..self.id_size]
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BindPair {
    pub(crate) in_index: u64,
    pub(crate) out_index: u64,
}

/// Iterator that yields coders in their processing order within a block.
///
/// Coders are chained together in blocks, and this iterator follows the chain
/// from the first coder to the last in their proper execution order.
pub struct OrderedCoderIter<'a> {
    block: &'a Block,
    current: Option<u64>,
    remaining: usize,
}

impl<'a> OrderedCoderIter<'a> {
    fn new(block: &'a Block) -> Self {
        let current = block.packed_streams.first().copied();
        Self {
            block,
            current,
            remaining: block.coders.len(),
        }
    }
}

impl<'a> Iterator for OrderedCoderIter<'a> {
    type Item = (usize, &'a Coder);

    fn next(&mut self) -> Option<Self::Item> {
        // An acyclic coder chain visits each coder at most once, so more than
        // `coders.len()` yields means the bind pairs form a cycle. Stop instead of
        // yielding forever (which would nest decoders until memory is exhausted); the
        // caller then fails cleanly when the truncated decode stack produces bad data.
        if self.remaining == 0 {
            return None;
        }
        let i = self.current?;
        self.current = self
            .block
            .find_bind_pair_for_out_stream(i)
            .map(|bp| bp.in_index);
        let item = self
            .block
            .coders
            .get(i as usize)
            .map(|item| (i as usize, item));
        if item.is_some() {
            self.remaining -= 1;
        }
        item
    }
}
