// Compile the scripts into the internal representation of commands
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::command::{
    Address, CaseConversion, CharacterMode, Command, CommandData, ParsedTransliteration,
    ProcessingContext, RegexMode, ReplacementPart, ReplacementTemplate, Substitution,
    Transliteration,
};
use crate::sed::delimited_parser::{
    ERR_UNTERMINATED_ADDRESS_REGEX, os_string_from_bytes, parse_char_escape, parse_regex_for_mode,
    parse_transliteration_for_mode, push_script_char,
};
use crate::sed::error_handling::{
    ScriptLocation, compilation_err, compilation_err_at, compilation_error, semantic_err,
    semantic_error,
};
use crate::sed::fast_regex::Regex;
use crate::sed::gnu_regex;
use crate::sed::named_writer::NamedWriter;
use crate::sed::script_char_provider::ScriptCharProvider;
use crate::sed::script_line_provider::{ScriptLineProvider, ScriptValue};

use std::cell::RefCell;
use std::mem;
use std::path::PathBuf;
use std::rc::Rc;

use uucore::error::{UResult, USimpleError};

const ERR_ADDRESS_0_USAGE: &str = "invalid usage of line address 0";
// GNU sed's words for these errors, as for the others; cash's sed had its own for the
// same errors.
const ERR_UNTERMINATED_S: &str = "unterminated `s' command";
const ERR_UNTERMINATED_Y: &str = "unterminated `y' command";
const ERR_TEXT_EXPECTED: &str = "expected \\ after `a', `c' or `i'";
const ERR_SANDBOX: &str = "e/r/w commands disabled in sandbox mode";

const ERR_UNKNOWN_OPTION_TO_S: &str = "unknown option to `s'";
const ERR_TRANSLITERATION_LENGTH: &str = "strings for `y' command are different lengths";

// Handling required after processing a command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandHandling {
    GetNext,  // Get next command and process that: !
    Return,   // Return from the sequence parser: }
    Continue, // Continue sequence parsing: all other commands
}

/// The type of functions that compile individual commands
type CommandHandler = fn(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling>;

// Command specification
#[derive(Debug, Clone, Copy)]
struct CommandSpec {
    n_addr: usize,           // Number of supported addresses
    handler: CommandHandler, // Argument-specific command compilation handler
}

/// Compile the scripts into an executable data structure.
pub fn compile(
    scripts: Vec<ScriptValue>,
    context: &mut ProcessingContext,
) -> UResult<Option<Rc<RefCell<Command>>>> {
    let mut make_providers = ScriptLineProvider::new(scripts);

    let mut empty_line = ScriptCharProvider::new("");
    let result = compile_sequence(&mut make_providers, &mut empty_line, context)?;

    // Comment-out the following to show the compiled script.
    #[cfg(any())]
    dbg!(&result);

    // Link branch commands to the target label commands.
    populate_label_map(result.clone(), context)?;
    populate_range_commands(result.clone(), context);
    resolve_branch_targets(result.clone(), context)?;

    // Link the ends of command blocks to their following commands.
    // This converts the tree into a graph, so it must be the last
    // conversion that traverses the structure as a tree.
    if context.parsed_block_nesting > 0 {
        return Err(USimpleError::new(1, "unmatched `{'"));
    }
    patch_block_endings(result.clone());

    Ok(result)
}

/// For every Command in the top-level `head` chain, look for
/// `CommandData::BranchTarget(Some(sub_head))` '{' commands.
/// Recursively patch the sub-chain, then splice its tail back to the
/// original “next” pointer of the *parent* (falling back to its own
/// parent_next if its own next was `None`).
fn patch_block_endings(head: Option<Rc<RefCell<Command>>>) {
    fn patch_block_endings_to_parent(
        mut cur: Option<Rc<RefCell<Command>>>,
        parent_next: Option<Rc<RefCell<Command>>>,
    ) {
        while let Some(rc_cmd) = cur {
            // Borrow mutably just long enough to inspect/rewire this node
            let cmd = rc_cmd.borrow_mut();
            // Save this node’s own next pointer
            let own_next = cmd.next.clone();
            // Decide what “splice target” to use:
            //   - if this node has its own_next, use that
            //   - otherwise, fall back to parent_next
            let splice_target = own_next.clone().or(parent_next.clone());

            // If it has a sub-block, recurse and then patch its tail
            if let CommandData::BranchTarget(Some(ref sub_head)) = cmd.data
                && cmd.code == '{'
            {
                // 1) recurse into the sub-chain, passing splice_target
                patch_block_endings_to_parent(Some(sub_head.clone()), splice_target.clone());

                // 2) find the tail of that sub-chain
                let mut tail = sub_head.clone();
                loop {
                    let next_in_sub = tail.borrow().next.clone();
                    match next_in_sub {
                        Some(n) => tail = n,
                        None => break,
                    }
                }

                // 3) splice the tail’s `.next` to splice_target
                tail.borrow_mut().next.clone_from(&splice_target);
            }

            // drop the borrow before moving on
            drop(cmd);

            // advance to the next sibling in this level
            cur = own_next;
        }
    }

    // top-level has no parent, so pass None
    patch_block_endings_to_parent(head, None);
}

/// Populate the context's label map with references to associated commands.
fn populate_label_map(
    mut cur: Option<Rc<RefCell<Command>>>,
    context: &mut ProcessingContext,
) -> UResult<()> {
    while let Some(rc_cmd) = cur.take() {
        // Borrow mutably just long enough to inspect/rewire this node
        let cmd = rc_cmd.borrow_mut();

        // Extract any label to insert after borrow ends
        let maybe_label = match &cmd.data {
            CommandData::BranchTarget(Some(sub_head)) => {
                populate_label_map(Some(sub_head.clone()), context)?;
                None
            }
            CommandData::Label(Some(label)) => Some(label.clone()),
            _ => None,
        };

        if let Some(label) = maybe_label
            && cmd.code == ':'
        {
            if context.label_to_command_map.contains_key(&label) {
                return semantic_error(&cmd.location, format!("duplicate label `{label}'"));
            }
            context.label_to_command_map.insert(label, rc_cmd.clone());
        }

        cur.clone_from(&cmd.next);
    }
    Ok(())
}

/// Populate the context's address range command list with references to associated commands.
fn populate_range_commands(mut cur: Option<Rc<RefCell<Command>>>, context: &mut ProcessingContext) {
    while let Some(rc_cmd) = cur.take() {
        // Borrow mutably just long enough to inspect/rewire this node
        let cmd = rc_cmd.borrow_mut();

        // Recursively process blocks.
        if let CommandData::BranchTarget(Some(sub_head)) = &cmd.data {
            populate_range_commands(Some(Rc::clone(sub_head)), context);
        }

        if cmd.addr2.is_some() {
            // Save detected range command.
            context.range_commands.push(Rc::clone(&rc_cmd));
        }

        cur.clone_from(&cmd.next);
    }
}

/// Replace branch labels with references to the corresponding commands.
/// Raise an error on undefined labels.
fn resolve_branch_targets(
    mut cur: Option<Rc<RefCell<Command>>>,
    context: &mut ProcessingContext,
) -> UResult<()> {
    while let Some(rc_cmd) = cur.take() {
        // Borrow mutably just long enough to inspect/rewire this node
        let mut cmd = rc_cmd.borrow_mut();

        // Recurse into blocks
        if let CommandData::BranchTarget(Some(sub_head)) = &cmd.data {
            resolve_branch_targets(Some(sub_head.clone()), context)?;
        }

        // Only for 't', 'T', or 'b' commands:
        if matches!(cmd.code, 't' | 'T' | 'b') {
            // Take ownership of the current data
            let old_data = mem::replace(&mut cmd.data, CommandData::None);

            // Build the replacement
            let new_data = match old_data {
                CommandData::Label(Some(label)) => {
                    let target = context
                        .label_to_command_map
                        .get(&label)
                        .cloned()
                        .ok_or_else(|| {
                            // GNU sed's words; cash's sed said "undefined label".
                            semantic_err(
                                &cmd.location,
                                format!("can't find label for jump to `{label}'"),
                            )
                        })?;
                    CommandData::BranchTarget(Some(target))
                }
                CommandData::Label(None) => CommandData::BranchTarget(None),
                other => other, // put back anything else unchanged
            };

            // Store it back
            cmd.data = new_data;
        }

        // Advance to the next sibling
        cur.clone_from(&cmd.next);
    }
    Ok(())
}

/// Compile provided scripts into a sequence of commands.
fn compile_sequence(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    context: &mut ProcessingContext,
) -> UResult<Option<Rc<RefCell<Command>>>> {
    let mut head: Option<Rc<RefCell<Command>>> = None;
    let mut tail: Option<Rc<RefCell<Command>>> = None;

    loop {
        line.eat_spaces();

        // Special first-line comment disabling default output
        // According to POSIX: "If the first two characters in the script are
        // "#n", the default output shall be suppressed".
        if !line.eol()
            && line.current() == '#'
            && lines.get_line_number() == 1
            && line.get_pos() == 0
        {
            line.advance();
            if !line.eol() && line.current() == 'n' {
                context.quiet = true;
            }
            // Ignore rest of line
            while !line.eol() {
                line.advance();
            }
        }

        // Empty lines and comments
        if line.eol() || line.current() == '#' {
            match lines.next_line()? {
                None => {
                    return Ok(head);
                }
                Some(line_bytes) => {
                    *line = ScriptCharProvider::new(line_bytes);
                }
            }
            continue;
        } else if line.current() == ';' {
            line.advance();
            continue;
        }

        let mut cmd = Rc::new(RefCell::new(Command::at_position(lines, line)));
        let n_addr = compile_address_range(lines, line, &mut cmd, context)?;
        line.eat_spaces();
        let mut cmd_spec = get_verified_cmd_spec(lines, line, n_addr, context.posix)?;
        // Compile the command according to its specification.
        let mut cmd_mut = cmd.borrow_mut();
        cmd_mut.code = line.current();
        match (cmd_spec.handler)(lines, line, &mut cmd_mut, context)? {
            CommandHandling::GetNext => {
                cmd_spec = get_verified_cmd_spec(lines, line, n_addr, context.posix)?;
                cmd_mut.code = line.current();
                (cmd_spec.handler)(lines, line, &mut cmd_mut, context)?;
            }
            CommandHandling::Return => return Ok(head),
            CommandHandling::Continue => (),
        }
        drop(cmd_mut);

        if let Some(ref t) = tail {
            // there's already a tail: link it
            t.borrow_mut().next = Some(cmd.clone());
        } else {
            // first element: set head
            head = Some(cmd.clone());
        }
        tail = Some(cmd);
    }
}

/// Return true if c is a valid character for specifying a context address
fn is_address_char(c: char) -> bool {
    matches!(c, '0'..='9' | '/' | '\\' | '$')
}

/// Compile a command's optional address range into cmd.
/// Return the number of addresses encountered.
///
/// As GNU sed reads them: `first~step` is one address (`compile_address`), so it may
/// start a range (`1~3,5p`), and a `~` after any other address is left to be the
/// command (`$~2p` is an unknown command `~`). The step was taken as a second address
/// after any first one, so `1~3,5p` was refused and `$~2p` taken.
fn compile_address_range(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Rc<RefCell<Command>>,
    context: &ProcessingContext,
) -> UResult<usize> {
    let mut n_addr = 0;
    let mut cmd = cmd.borrow_mut();

    line.eat_spaces();
    if !line.eol() && matches!(line.current(), '+' | '~') && !context.posix {
        // `+N` and `~N` end a range; they cannot start one.
        line.advance();
        return compilation_error(lines, line, "invalid usage of +N or ~N as first address");
    }
    if !line.eol() && is_address_char(line.current()) {
        cmd.addr1 = Some(compile_address(lines, line, context)?);
        n_addr += 1;
    }

    line.eat_spaces();
    if n_addr == 1 && !line.eol() && line.current() == ',' {
        line.advance();
        line.eat_spaces();
        if !line.eol() && line.current() == '~' && !context.posix {
            // E.g. /foo/,~10: Start at foo, include all lines until multiple of 10 is
            // reached. GNU sed reads the step as a number that may be missing, which
            // counts as 0, and leaves whatever follows to the command: `2,~p` is just
            // line 2.
            line.advance();
            line.eat_spaces();
            let step = parse_number(lines, line, false)?.unwrap_or(0);
            cmd.addr2 = Some(Address::StepEnd(step));
        } else {
            // What follows a comma has to be an address, as GNU sed has it: `1,xp` is
            // its "unexpected `,'", and so is `1,+2p` in POSIX mode.
            if line.eol()
                || !(is_address_char(line.current()) || (line.current() == '+' && !context.posix))
            {
                return compilation_error(lines, line, "unexpected `,'");
            }
            cmd.addr2 = Some(compile_address(lines, line, context)?);
        }
        n_addr += 1;
    }

    // Line 0 starts a range that a regular expression ends, so that the expression can
    // match the first line, a GNU extension; and `0r` reads its file before the first
    // line. Anything else, and anything in POSIX mode, is GNU's error.
    if matches!(cmd.addr1, Some(Address::Line(0))) {
        line.eat_spaces();
        let allowed = match &cmd.addr2 {
            Some(Address::Re(_)) => true,
            Some(_) => false,
            // After the address, spaces are eaten: the command is at the position.
            None => !line.eol() && line.current() == 'r',
        };
        if !allowed || context.posix {
            return compilation_error(lines, line, ERR_ADDRESS_0_USAGE);
        }
    }

    Ok(n_addr)
}

/// Read the line's remaining characters as a file path and return it.
// TODO Move to delimited_parser in separate commit.
fn read_file_path(lines: &ScriptLineProvider, line: &mut ScriptCharProvider) -> UResult<PathBuf> {
    line.advance(); // Skip the command/w character
    line.eat_spaces(); // Skip any leading whitespace

    let mut path = Vec::new();
    while !line.eol() {
        path.push(line.current_byte());
        line.advance();
    }

    if path.is_empty() {
        compilation_error(lines, line, "missing filename in r/R/w/W commands")
    } else {
        os_string_from_bytes(path)
            .map(PathBuf::from)
            .map_err(|e| compilation_err(lines, line, format!("invalid characters file path: {e}")))
    }
}

/// Compile and return a single range address specification.
// The `~` forms are read by compile_address_range() itself.
fn compile_address(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
    context: &ProcessingContext,
) -> UResult<Address> {
    let mut icase = false;
    let mut multiline = false;

    if line.eol() {
        return compilation_error(lines, line, "expected context address");
    }

    match line.current() {
        '\\' | '/' => {
            // Regular expression
            if line.current() == '\\' {
                // The next character is an arbitrary delimiter
                line.advance();
            }
            let regex_mode = if context.regex_extended {
                RegexMode::Extended
            } else {
                RegexMode::Basic
            };
            let re = parse_regex_for_mode(
                lines,
                line,
                regex_mode,
                context.character_mode,
                ERR_UNTERMINATED_ADDRESS_REGEX,
            )?;
            // Skip over delimiter
            line.advance();

            // GNU sed's flags `I` and `M`, in any number and order, blanks around them;
            // only one `I` was taken, and no `M` (`/a/Mp` was the unknown command `M`).
            // POSIX mode has none.
            loop {
                line.eat_spaces();
                if line.eol() || context.posix {
                    break;
                }
                match line.current() {
                    'I' => icase = true,
                    'M' => multiline = true,
                    _ => break,
                }
                line.advance();
            }

            // GNU sed has read up to the command, and reports a bad expression there.
            let column = line.get_pos();
            Ok(Address::Re(compile_regex(
                lines, column, &re, context, icase, multiline,
            )?))
        }
        '$' => {
            line.advance();
            Ok(Address::Last)
        }
        '+' => {
            // As in GNU sed, blanks may follow the `+`, and a missing number counts as
            // 0: `2,+p` is line 2 alone.
            line.advance();
            line.eat_spaces();
            let number = parse_number(lines, line, false)?.unwrap_or(0);
            Ok(Address::RelLine(number))
        }
        c if c.is_ascii_digit() => {
            let number = parse_required_number(lines, line)?;
            // `first~step`, with blanks allowed around the `~`, a GNU extension: in POSIX
            // mode the `~` is left to be the command, an unknown one, as in GNU sed. A
            // missing step counts as 0, and `first~0` is the line `first` alone.
            if !context.posix {
                line.eat_spaces();
                if !line.eol() && line.current() == '~' {
                    line.advance();
                    line.eat_spaces();
                    let step = parse_number(lines, line, false)?.unwrap_or(0);
                    if step != 0 {
                        return Ok(Address::Step {
                            first: number,
                            step,
                        });
                    }
                }
            }
            Ok(Address::Line(number))
        }
        // compile_address_range() calls this only on what can start an address.
        _ => compilation_error(lines, line, "expected context address"),
    }
}

/// Parse and return the decimal number that must be at the current line position.
fn parse_required_number(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
) -> UResult<usize> {
    parse_number(lines, line, true)?.ok_or_else(|| compilation_err(lines, line, "number expected"))
}

/// Parse and return the decimal number at the current line position.
/// Advance the line to first non-digit or EOL.
/// Issue an error if the number is required.
fn parse_number(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
    required: bool,
) -> UResult<Option<usize>> {
    let mut num_str = String::new();

    while !line.eol() && line.current().is_ascii_digit() {
        num_str.push(line.current());
        line.advance();
    }

    if num_str.is_empty() {
        if required {
            return compilation_error(lines, line, "number expected");
        }
        return Ok(None);
    }

    num_str
        .parse::<usize>()
        .map_err(|_| format!("invalid number '{num_str}'"))
        .map_err(|msg| compilation_err(lines, line, msg))
        .map(Some)
}

/// Parse the end of a command, failing with an error on extra characters.
fn parse_command_ending(lines: &ScriptLineProvider, line: &mut ScriptCharProvider) -> UResult<()> {
    if !line.eol() && line.current() == ';' {
        line.advance();
        return Ok(());
    }

    // A `}` or a comment may follow a command, as in GNU sed, which leaves them to be
    // read next: `p#c` was refused.
    if !line.eol() && matches!(line.current(), '}' | '#') {
        return Ok(());
    }

    if !line.eol() {
        return compilation_error(lines, line, "extra characters after command");
    }

    Ok(())
}

/// Convert a primitive BRE pattern to a safe ERE-compatible pattern.
/// - Replaces `\(`, `\)`, `\?`, `\+`, `\|`, `\{` and `\}` with `(`, `)`, `?`, `+`, `|`, `{` and `}`.
/// - Puts single-digit back-references in non-capturing groups..
/// - Escapes ERE-only metacharacters: `+ ? { } | ( )`.
/// - Leaves all other bytes as-is.
///
/// Where GNU sed's BRE has a character, not an operator, so does the result:
/// - `*`, `\+` and `\?` where an expression starts: at the start, after `\(`, `\|` or
///   an anchor (`*a`, `\(*a\)`, `^*a`). A leading `*` was a repetition of nothing, an
///   error, and `^*` the `^` repeated.
/// - `^` but at the start and after `\(` and `\|`, where it is an anchor, and `$` but
///   at the end and before `\)` and `\|`. `\(^a\)` and `\(a$\)` matched `^` and `$`.
/// - With `posix`, `\|`, `\+` and `\?`, which POSIX does not have.
///
/// A bracket expression is copied as it is.
fn bre_to_ere(pattern: &[u8], posix: bool) -> Vec<u8> {
    let mut result = Vec::with_capacity(pattern.len());
    let mut pos = 0;

    // Whether an expression starts here, so that a repetition is a character.
    let mut expression_start = true;
    // Whether a `^` here is an anchor.
    let mut caret_here = true;
    while let Some(&c) = pattern.get(pos) {
        pos += 1;
        let (starts_expression, anchors_caret) = if c == b'\\' {
            let next = pattern.get(pos).copied();
            if next.is_some() {
                pos += 1;
            }
            match next {
                Some(b'(') => {
                    result.push(b'('); // Group start
                    (true, true)
                }
                Some(b')') => {
                    result.push(b')'); // Group end
                    (false, false)
                }
                Some(b'|') if !posix => {
                    result.push(b'|'); // Alternation operator
                    (true, true)
                }
                Some(op @ (b'?' | b'+')) if !posix && !expression_start => {
                    result.push(op); // Quantifier 0 or 1, 1 or more
                    (false, false)
                }
                Some(b'{') => {
                    result.push(b'{'); // Brace quantifier start
                    (false, false)
                }
                Some(b'}') => {
                    result.push(b'}'); // Brace quantifier end
                    (false, false)
                }
                Some(v) if v.is_ascii_digit() => {
                    // Back-reference.  In sed BREs these are single-digit
                    // (\1-\9) whereas fancy_regex supports multi-digit
                    // back-references. Put them in a non-capturing group
                    // to avoid having the number extend beyond the single
                    // digit. Example: In sed \11 matches group 1 followed
                    // by '1', not group 11.
                    result.extend_from_slice(b"(?:\\");
                    result.push(v);
                    result.push(b')');
                    (false, false)
                }
                Some(next) => {
                    // Preserve other escaped characters; an anchor among them (`\b`,
                    // `\<`, and `\A` and `\z` for GNU's `` \` `` and `\'`) starts an
                    // expression.
                    result.push(b'\\');
                    result.push(next);
                    (
                        matches!(next, b'b' | b'B' | b'<' | b'>' | b'A' | b'z'),
                        false,
                    )
                }
                None => {
                    // Trailing backslash; keep it.
                    result.push(b'\\');
                    (false, false)
                }
            }
        } else {
            match c {
                b'[' => {
                    pos = copy_bracket(pattern, pos, &mut result);
                    (false, false)
                }
                b'*' if expression_start => {
                    result.extend_from_slice(b"\\*");
                    (false, false)
                }
                b'+' | b'?' | b'{' | b'}' | b'|' | b'(' | b')' => {
                    // Escape unsupported ERE metacharacters.
                    result.push(b'\\');
                    result.push(c);
                    (false, false)
                }
                b'^' if caret_here => {
                    result.push(c);
                    (true, false)
                }
                b'^' => {
                    // In BREs ^ has special meaning at the beginning, after `\(` and
                    // `\|`, and as bracket negation. Other uses are literal, though
                    // valid in EREs: "the ERE "a^b" is valid, but can never match
                    // because the 'a' prevents the expression "^b" from matching
                    // starting at the first character." POSIX 9.4.9 ERE Expression
                    // Anchoring
                    result.extend_from_slice(b"\\^");
                    (false, false)
                }
                b'$' if !dollar_anchors(pattern, pos, posix) => {
                    // Similarly for $ appearing not at the end.
                    result.extend_from_slice(b"\\$");
                    (false, false)
                }
                _ => {
                    result.push(c);
                    (false, false)
                }
            }
        };
        expression_start = starts_expression;
        caret_here = anchors_caret;
    }

    result
}

/// Whether a `$` before `pos` is an anchor in a BRE: at the end, or before `\)` or `\|`.
fn dollar_anchors(pattern: &[u8], pos: usize, posix: bool) -> bool {
    match pattern.get(pos..) {
        None | Some([]) => true,
        Some([b'\\', b')', ..]) => true,
        Some([b'\\', b'|', ..]) => !posix,
        Some(_) => false,
    }
}

/// Copy the bracket expression whose `[` is just before `pos` and return where it ends.
/// It ends at a `]` that is not its first character (after a `^`), outside `[:`, `[.`
/// and `[=` and their closing `:]`, `.]` and `=]`, and not escaped: sed's parser has
/// left escapes such as `\]` for the engine.
fn copy_bracket(pattern: &[u8], mut pos: usize, result: &mut Vec<u8>) -> usize {
    result.push(b'[');
    if pattern.get(pos) == Some(&b'^') {
        result.push(b'^');
        pos += 1;
    }
    if pattern.get(pos) == Some(&b']') {
        result.push(b']');
        pos += 1;
    }
    while let Some(&c) = pattern.get(pos) {
        pos += 1;
        result.push(c);
        match c {
            b']' => break,
            b'\\' => {
                if let Some(&next) = pattern.get(pos) {
                    result.push(next);
                    pos += 1;
                }
            }
            b'[' if matches!(pattern.get(pos), Some(b':' | b'.' | b'=')) => {
                let delimiter = pattern.get(pos).copied().unwrap_or_default();
                result.push(delimiter);
                pos += 1;
                while let Some(&inner) = pattern.get(pos) {
                    pos += 1;
                    result.push(inner);
                    if inner == delimiter && pattern.get(pos) == Some(&b']') {
                        result.push(b']');
                        pos += 1;
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    pos
}

// Whether a regular expression names a carriage return: a literal CR byte, or the
/// escapes `\r` and `\x0D` (an escaped backslash does not count).
fn mentions_carriage_return(pattern: &[u8]) -> bool {
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\r' => return true,
            b'\\' => {
                let rest = &pattern[i + 1..];
                let hex_cr = |digits: &[u8]| {
                    std::str::from_utf8(digits)
                        .ok()
                        .and_then(|d| u32::from_str_radix(d, 16).ok())
                        == Some(0x0D)
                };
                if rest.first() == Some(&b'r')
                    || (rest.first() == Some(&b'x') && rest.len() >= 3 && hex_cr(&rest[1..3]))
                {
                    return true;
                }
                // Skip the escaped character, so `\\r` is a backslash and an `r`.
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Compile the provided regular expression string into a corresponding engine.
/// An empty pattern results in None, which means that the last RE employed
/// at runtime will be used.
///
/// An error is reported at `column` of the current script line: GNU sed compiles an
/// expression once it has read the command or address that holds it, flags and all,
/// and says so where that ends. One `regcomp` refuses is in its words (`gnu_regex`);
/// the engine's own words were reported, where the expression ended.
fn compile_regex(
    lines: &ScriptLineProvider,
    column: usize,
    pattern: impl AsRef<[u8]>,
    context: &ProcessingContext,
    icase: bool,
    multiline: bool,
) -> UResult<Option<Regex>> {
    let pattern = pattern.as_ref();
    if pattern.is_empty() {
        return Ok(None);
    }

    let syntax = gnu_regex::Syntax {
        extended: context.regex_extended,
        posix: context.posix,
        utf8: context.character_mode == CharacterMode::Utf8,
    };
    if let Some(error) = gnu_regex::syntax_error(pattern, syntax) {
        return Err(compilation_err_at(lines, column, error));
    }

    if mentions_carriage_return(pattern) {
        context.cr_in_script.set(true);
    }

    // Convert basic to extended regular expression if needed.
    let pattern = if context.regex_extended {
        pattern.to_vec()
    } else {
        bre_to_ere(pattern, context.posix)
    };

    // Add any required modifiers.
    let mut modifiers = Vec::new();
    if icase {
        modifiers.push(b'i');
    }
    if multiline {
        modifiers.push(b'm');
    }
    let pattern = if modifiers.is_empty() {
        pattern
    } else {
        // Append modifiers.
        let mut with_modifiers = Vec::with_capacity(pattern.len() + modifiers.len() + 3);
        with_modifiers.extend_from_slice(b"(?");
        with_modifiers.extend_from_slice(&modifiers);
        with_modifiers.push(b')');
        with_modifiers.extend_from_slice(&pattern);
        with_modifiers
    };

    // Compile into engine.
    let compiled = Regex::new(&pattern, context.character_mode).map_err(|e| {
        compilation_err_at(
            lines,
            column,
            format!("invalid regex '{}': {e}", String::from_utf8_lossy(&pattern)),
        )
    })?;

    Ok(Some(compiled))
}

/// Compile a regular expression replacement string according to character mode.
/// With `case_conversion` (GNU, not --posix), `\U`, `\L`, `\E`, `\u` and `\l` convert
/// the case of what follows.
pub fn compile_replacement(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    character_mode: CharacterMode,
    case_conversion: bool,
) -> UResult<ReplacementTemplate> {
    let mut parts = Vec::new();
    let mut literal = Vec::new();

    let delimiter = line.current();
    line.advance();

    loop {
        while !line.eol() {
            match line.current() {
                '\\' => {
                    line.advance();

                    // Line input_action
                    if line.eol() {
                        if let Some(next_line) = lines.next_line()? {
                            literal.push(b'\n');
                            *line = ScriptCharProvider::new(next_line);
                            continue;
                        }
                        return compilation_error(lines, line, ERR_UNTERMINATED_S);
                    }

                    match line.current() {
                        // \0 - \9
                        c @ '0'..='9' => {
                            let ref_num = u32::from(c) - u32::from('0');

                            if !literal.is_empty() {
                                parts.push(ReplacementPart::Literal(std::mem::take(&mut literal)));
                            }
                            if ref_num == 0 {
                                parts.push(ReplacementPart::WholeMatch);
                            } else {
                                parts.push(ReplacementPart::Group(ref_num));
                            }
                            line.advance();
                        }

                        // Literal \ and &
                        '\\' | '&' => {
                            literal.push(line.current_byte());
                            line.advance();
                        }

                        // Literal delimiter
                        v if v == delimiter => {
                            literal.push(line.current_byte());
                            line.advance();
                        }

                        // GNU's case conversions, which --posix leaves out.
                        c @ ('U' | 'L' | 'E' | 'u' | 'l') if case_conversion => {
                            if !literal.is_empty() {
                                parts.push(ReplacementPart::Literal(std::mem::take(&mut literal)));
                            }
                            parts.push(ReplacementPart::Case(match c {
                                'U' => CaseConversion::Upper,
                                'L' => CaseConversion::Lower,
                                'u' => CaseConversion::UpperNext,
                                'l' => CaseConversion::LowerNext,
                                _ => CaseConversion::End,
                            }));
                            line.advance();
                        }

                        // other escape sequences
                        _ => {
                            if let Some(decoded) = parse_char_escape(line) {
                                push_script_char(&mut literal, decoded, character_mode);
                            } else {
                                // A backslash before a character with no escape of its own
                                // stands for the character, as in GNU sed: `s/a/\q/` gives
                                // `q`, and `\U` under --posix is `U`. The backslash was
                                // kept.
                                literal.push(line.current_byte());
                                line.advance();
                            }
                        }
                    }
                }

                '&' => {
                    if !literal.is_empty() {
                        parts.push(ReplacementPart::Literal(std::mem::take(&mut literal)));
                    }
                    parts.push(ReplacementPart::WholeMatch);
                    line.advance();
                }

                '\n' => {
                    return compilation_error(
                        lines,
                        line,
                        "unescaped newline inside substitute replacement",
                    );
                }

                c if c == delimiter => {
                    line.advance(); // skip closing delimiter
                    if !literal.is_empty() {
                        parts.push(ReplacementPart::Literal(literal));
                    }
                    return Ok(ReplacementTemplate::new(parts).with_character_mode(character_mode));
                }

                _ => {
                    literal.push(line.current_byte());
                    line.advance();
                }
            }
        }

        // Fetch next line for continued replacement string
        if let Some(next_line) = lines.next_line()? {
            *line = ScriptCharProvider::new(next_line);
        } else {
            return compilation_error(lines, line, ERR_UNTERMINATED_S);
        }
    }
}

// Handles s
fn compile_subst_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // move past 's'

    // An `s` that ends the line has no pattern; reading its delimiter there panicked.
    if line.eol() {
        return compilation_error(lines, line, ERR_UNTERMINATED_S);
    }
    let delimiter = line.current();
    if delimiter == '\0' || delimiter == '\\' {
        return compilation_error(
            lines,
            line,
            "substitute pattern cannot be delimited by newline or backslash",
        );
    }

    let regex_mode = if context.regex_extended {
        RegexMode::Extended
    } else {
        RegexMode::Basic
    };
    let pattern = parse_regex_for_mode(
        lines,
        line,
        regex_mode,
        context.character_mode,
        ERR_UNTERMINATED_S,
    )?;
    let mut subst = Box::new(Substitution::default());

    subst.replacement = compile_replacement(lines, line, context.character_mode, !context.posix)?;
    compile_subst_flags(lines, line, &mut subst, context.posix, context.sandbox)?;

    // GNU sed reports what is wrong with the expression once it has read the flags, a
    // `;` that ends them too, but not a `}` or `#`.
    let column = if !line.eol() && line.current() == ';' {
        line.get_pos() + 1
    } else {
        line.get_pos()
    };

    if pattern.is_empty() && (subst.ignore_case || subst.multiline) {
        return Err(compilation_err_at(
            lines,
            column,
            "cannot specify modifiers on empty regexp",
        ));
    }

    // Compile regex with now known modifier flags.
    subst.regex = compile_regex(
        lines,
        column,
        &pattern,
        context,
        subst.ignore_case,
        subst.multiline,
    )?;

    // Catch invalid group references at compile time, if possible.
    if let Some(regex) = &subst.regex
        && subst.replacement.max_group_number > regex.captures_len() - 1
    {
        return Err(compilation_err_at(
            lines,
            column,
            format!(
                "invalid reference \\{} on `s' command's RHS",
                subst.replacement.max_group_number
            ),
        ));
    }
    cmd.data = CommandData::Substitution(subst);

    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Continue)
}

// Handles y
fn compile_trans_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // move past 'y'

    // A `y` that ends the line has no strings; reading its delimiter there panicked.
    if line.eol() {
        return compilation_error(lines, line, ERR_UNTERMINATED_Y);
    }
    let delimiter = line.current();
    if delimiter == '\0' || delimiter == '\\' {
        return compilation_error(
            lines,
            line,
            "transliteration string cannot be delimited by newline or backslash",
        );
    }

    let source = parse_transliteration_for_mode(lines, line, context.character_mode)?;
    let target = parse_transliteration_for_mode(lines, line, context.character_mode)?;
    let source_has_cr = match &source {
        ParsedTransliteration::Bytes(bytes) => bytes.contains(&b'\r'),
        ParsedTransliteration::Text(text) => text.contains('\r'),
    };
    if source_has_cr {
        context.cr_in_script.set(true);
    }
    // Both strings are read in the one character mode, so they are text together or
    // bytes together; were they not, bytes would serve, as for byte mode.
    let transliteration = match (source, target) {
        (ParsedTransliteration::Text(source), ParsedTransliteration::Text(target)) => {
            if source.chars().count() != target.chars().count() {
                return compilation_error(lines, line, ERR_TRANSLITERATION_LENGTH);
            }
            Box::new(Transliteration::from_strings(&source, &target))
        }
        (source, target) => {
            let (source, target) = (source.into_bytes(), target.into_bytes());
            if source.len() != target.len() {
                return compilation_error(lines, line, ERR_TRANSLITERATION_LENGTH);
            }
            Box::new(Transliteration::from_bytes(&source, &target))
        }
    };
    cmd.data = CommandData::Transliteration(transliteration);

    line.advance(); // move past last delimiter
    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Continue)
}

/// Parse the substitution command's optional flags
pub fn compile_subst_flags(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
    subst: &mut Substitution,
    posix: bool,
    sandbox: bool,
) -> UResult<()> {
    let mut seen_g = false;
    let mut seen_number = false;

    subst.occurrence = 1; // default
    subst.global = false;
    subst.print_flag = false;
    subst.p_before_e = false;
    subst.ignore_case = false;
    subst.execute = false;
    subst.multiline = false;
    subst.write_file = None;

    loop {
        line.eat_spaces();
        if line.eol() {
            break;
        }

        match line.current() {
            'g' => {
                if seen_g {
                    return compilation_error(lines, line, "multiple `g' options to `s' command");
                }
                seen_g = true;
                subst.global = true;
                line.advance();
            }

            'p' => {
                // GNU sed's error; a second `p` was taken, and printed twice.
                if subst.print_flag {
                    return compilation_error(lines, line, "multiple `p' options to `s' command");
                }
                subst.print_flag = true;
                // 'p' is applied before 'e' iff 'e' has not been seen yet.
                subst.p_before_e = !subst.execute;
                line.advance();
            }

            'i' | 'I' => {
                if posix {
                    return compilation_error(lines, line, ERR_UNKNOWN_OPTION_TO_S);
                }
                subst.ignore_case = true;
                line.advance();
            }

            'm' | 'M' => {
                if posix {
                    return compilation_error(lines, line, ERR_UNKNOWN_OPTION_TO_S);
                }
                subst.multiline = true;
                line.advance();
            }

            'e' => {
                // As in GNU sed: POSIX has no `e` flag, and the sandbox runs no
                // command. Both had a message of cash's own.
                if posix {
                    return compilation_error(lines, line, ERR_UNKNOWN_OPTION_TO_S);
                }
                if sandbox {
                    return compilation_error(lines, line, ERR_SANDBOX);
                }
                subst.execute = true;
                line.advance();
            }

            '0' => {
                return compilation_error(
                    lines,
                    line,
                    "number option to `s' command may not be zero",
                );
            }

            _c @ '1'..='9' => {
                if seen_number {
                    return compilation_error(
                        lines,
                        line,
                        "multiple number options to `s' command",
                    );
                }

                // A number too big to hold is an occurrence no line has, as GNU sed
                // takes it; it was an error of cash's own.
                let mut number = 0usize;
                while !line.eol() {
                    let Some(digit) = line.current().to_digit(10) else {
                        break;
                    };
                    number = number.saturating_mul(10).saturating_add(digit as usize);
                    line.advance();
                }

                subst.occurrence = number;
                seen_number = true;
            }

            'w' => {
                if sandbox {
                    return compilation_error(lines, line, ERR_SANDBOX);
                }
                let location = ScriptLocation::at_position(lines, line);
                let path = read_file_path(lines, line)?;
                subst.write_file = Some(NamedWriter::new(path, location)?);
                return Ok(()); // 'w' is the last flag allowed
            }

            // A `}` or a comment ends the flags too, as in GNU sed: `{s/a/b/}` was
            // refused as an unknown option.
            ';' | '\n' | '}' | '#' => break,

            _ => {
                return compilation_error(lines, line, ERR_UNKNOWN_OPTION_TO_S);
            }
        }
    }

    Ok(())
}

// Handles }
fn compile_end_group_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    _cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    if context.parsed_block_nesting == 0 {
        return compilation_error(lines, line, "unexpected `}'");
    }
    context.parsed_block_nesting -= 1;
    line.advance();
    line.eat_spaces();
    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Return)
}

// Handles !
fn compile_negation_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    _context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    // At the second `!`, which GNU sed has read: past it the column was one too many.
    if cmd.non_select {
        return compilation_error(lines, line, "multiple `!'s");
    }
    line.advance();
    line.eat_spaces();
    cmd.non_select = true;
    Ok(CommandHandling::GetNext)
}

/// Compile a command that doesn't take any arguments
// Handles d D g G h H l n N p P q x =
fn compile_empty_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    _cmd: &mut Command,
    _context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // Skip the command character
    line.eat_spaces(); // Skip any trailing whitespace

    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Continue)
}

// Handles r
fn compile_read_file_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    if context.sandbox {
        return compilation_error(lines, line, ERR_SANDBOX);
    }
    let path = read_file_path(lines, line)?;
    cmd.data = CommandData::Path(path);
    Ok(CommandHandling::Continue)
}

// Handles R, a GNU extension: a line of the file at each run. As in GNU sed, the file is
// opened as the script is read, one that cannot be opened is no error (`R` then reads
// nothing), and every `R` that names it reads on from the same place.
fn compile_read_line_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    if context.sandbox {
        return compilation_error(lines, line, ERR_SANDBOX);
    }
    let path = read_file_path(lines, line)?;
    let file = context
        .line_files
        .entry(path)
        .or_insert_with_key(|path| {
            Rc::new(RefCell::new(
                std::fs::File::open(path).ok().map(std::io::BufReader::new),
            ))
        })
        .clone();
    cmd.data = CommandData::LineFile(file);
    Ok(CommandHandling::Continue)
}

// Handles w
fn compile_write_file_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    if context.sandbox {
        return compilation_error(lines, line, ERR_SANDBOX);
    }
    let location = ScriptLocation::at_position(lines, line);
    let path = read_file_path(lines, line)?;
    cmd.data = CommandData::NamedWriter(NamedWriter::new(path, location)?);
    Ok(CommandHandling::Continue)
}

// Handles {
fn compile_block_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // move past '{'
    context.parsed_block_nesting += 1;
    let block_body = compile_sequence(lines, line, context)?;
    cmd.data = CommandData::BranchTarget(block_body);
    Ok(CommandHandling::Continue)
}

// Handles b, t, :
fn compile_label_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    _context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    /// Return true if `c` is in the POSIX portable filename character set.
    fn is_portable_filename_char(c: char) -> bool {
        c.is_ascii_alphanumeric()  // A–Z, a–z, 0–9
        || matches!(c, '.' | '_' | '-')
    }

    line.advance(); // Skip the command character
    line.eat_spaces(); // Skip any leading whitespace

    let mut label = String::new();
    while !line.eol() && is_portable_filename_char(line.current()) {
        label.push(line.current());
        line.advance();
    }

    if label.is_empty() {
        if cmd.code == ':' {
            // GNU sed's words; cash's sed said "empty label". GNU sed has read up to
            // what ends the label, not that.
            return Err(compilation_err_at(
                lines,
                line.get_pos(),
                "\":\" lacks a label",
            ));
        }
        cmd.data = CommandData::Label(None);
    } else {
        cmd.data = CommandData::Label(Some(label));
    }

    // GNU sed ends a label at a blank too, and reads what follows as the next command:
    // `:x /\\$/ { N; s/\\\n//; bx }`. A label ended by `;` or the line's end is POSIX's.
    let ended_by_blank = !line.eol() && line.current().is_whitespace();
    line.eat_spaces(); // Skip any trailing whitespace
    if ended_by_blank && !line.eol() && !matches!(line.current(), ';' | '}' | '#') {
        return Ok(CommandHandling::Continue);
    }
    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Continue)
}

/// Compile commands that take a number as an argument.
// Handles l q Q
fn compile_number_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // Skip the command character
    line.eat_spaces(); // Skip any leading whitespace

    // A bare `q` or `Q` exits with 0. As in GNU sed, a bare `l` wraps at `-l N`
    // (default 70), not at the terminal's width.
    let n = match parse_number(lines, line, false)? {
        Some(n) => n,
        None if cmd.code == 'l' => context.length,
        None => 0,
    };
    cmd.data = CommandData::Number(n);

    line.eat_spaces(); // Skip any trailing whitespace
    parse_command_ending(lines, line)?;
    Ok(CommandHandling::Continue)
}

/// Compile commands that take text as an argument.
// Handles a, c, i
// According to POSIX, these commands expect \ followed by text.
// As a GNU extension the initial \ can be ommitted, and from then on
// character escapes are honored.
fn compile_text_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    line.advance(); // Skip the command character.
    line.eat_spaces(); // Skip any leading whitespace.
    if context.posix {
        compile_text_command_posix(lines, line, cmd, context)
    } else {
        compile_text_command_gnu(lines, line, cmd, context)
    }
}

/// Compile commands that take text as an argument (GNU syntax).
// Handles a, c, i; after the command and initial whitespace have been consumed.
// According to POSIX, these commands expect \ followed by text.
// As a GNU extension the initial \ can be ommitted, and from then on
// character escapes are honored.
fn compile_text_command_gnu(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    // True after a \ at the end of a line
    let mut escaped_newline = false;

    if line.eol() {
        return compilation_error(lines, line, ERR_TEXT_EXPECTED);
    }

    // Skip optional \.
    if !line.eol() && line.current() == '\\' {
        line.advance();
        escaped_newline = line.eol();
    }

    // Gather replacement text.  Stop on a non-escaped newline.
    let mut text = Vec::new();
    'text_content: loop {
        if escaped_newline {
            match lines.next_line()? {
                None => {
                    break 'text_content;
                }
                Some(line_bytes) => {
                    *line = ScriptCharProvider::new(line_bytes);
                }
            }
            escaped_newline = false;
        }

        // Non-escaped newline
        if line.eol() {
            text.push(b'\n');
            break 'text_content;
        }

        if line.current() == '\\' {
            line.advance();

            if line.eol() {
                escaped_newline = true;
                text.push(b'\n');
                continue 'text_content;
            }

            if let Some(decoded) = parse_char_escape(line) {
                push_script_char(&mut text, decoded, context.character_mode);
            } else {
                // Invalid escapes result in the escaped character.
                text.push(line.current_byte());
                line.advance();
            }
        } else {
            text.push(line.current_byte());
            line.advance();
        }
    }
    cmd.data = CommandData::Text(Rc::from(text));
    Ok(CommandHandling::Continue)
}

/// Compile commands that take text as an argument (POSIX syntax).
// Handles a, c, i; after the command and initial whitespace have been consumed.
// According to POSIX, these commands expect \ followed by text.
fn compile_text_command_posix(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    _context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    if line.eol() || line.current() != '\\' {
        return compilation_error(lines, line, ERR_TEXT_EXPECTED);
    }

    line.advance(); // Skip \.
    line.eat_spaces(); // Skip any whitespace at the end of \.
    if !line.eol() {
        return compilation_error(
            lines,
            line,
            format!(
                "extra characters after \\ at the end of `{}' command",
                cmd.code
            ),
        );
    }

    let mut text = Vec::new();
    while let Some(line) = lines.next_line()? {
        if line.ends_with(b"\\") {
            // Line ends with \ to escape \n; remove the trailing \.
            text.extend_from_slice(&line[..line.len() - 1]);
            text.push(b'\n');
        } else {
            text.extend_from_slice(&line);
            text.push(b'\n');
            break;
        }
    }

    if text.is_empty() {
        compilation_error(lines, line, "incomplete command")?;
    }

    cmd.data = CommandData::Text(Rc::from(text));
    Ok(CommandHandling::Continue)
}

// Handle v
fn compile_version_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    _cmd: &mut Command,
    _context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    // Claim version partify with GNU sed 4.9
    const GNU_MAJOR: u8 = 4;
    const GNU_MINOR: u8 = 9;
    const GNU_PATCH: u8 = 0;

    line.advance();
    line.eat_spaces(); // Skip any leading whitespace.

    let mut major = String::new();
    let mut minor = String::new();
    let mut patch = String::new();

    let mut ver_semantic = 0;

    while !line.eol() {
        if line.current() == '.' {
            ver_semantic += 1;
            line.advance();
        } else {
            match ver_semantic {
                0 => major.push(line.current()),
                1 => minor.push(line.current()),
                2 => patch.push(line.current()),
                _ => return compilation_error(lines, line, "invalid version of sed"),
            }
            line.advance();
        }
    }

    if major.is_empty() {
        major = GNU_MAJOR.to_string();
        minor = GNU_MINOR.to_string();
        patch = GNU_PATCH.to_string();
    }

    if minor.is_empty() {
        minor.push('0');
    }
    if patch.is_empty() {
        patch.push('0');
    }

    match major.parse::<u8>() {
        Ok(major_int) => match minor.parse::<u8>() {
            Ok(minor_int) => match patch.parse::<u8>() {
                Ok(patch_int) => {
                    // Versions compare part by part, as in GNU sed: 4.8.1 and 3.99 are
                    // older than 4.9, where the patch had to be 0 and the minor no more
                    // than 9 whatever the major.
                    if (major_int, minor_int, patch_int) <= (GNU_MAJOR, GNU_MINOR, GNU_PATCH) {
                        return Ok(CommandHandling::Continue);
                    }
                    compilation_error(lines, line, "expected newer version of sed")
                }
                Err(_) => compilation_error(lines, line, "invalid version of sed"),
            },
            Err(_) => compilation_error(lines, line, "invalid version of sed"),
        },
        Err(_) => compilation_error(lines, line, "invalid version of sed"),
    }
}

// Handles e
// With no argument, the command executes the pattern space as a shell
// command at runtime. With an argument, the rest of the line is the
// command to run, following the same escape and backslash-newline
// continuation rules as the GNU a/c/i text argument.
fn compile_execute_command(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    cmd: &mut Command,
    context: &mut ProcessingContext,
) -> UResult<CommandHandling> {
    // GNU sed's error, as for `r` and `w`; POSIX mode does not know the command at all
    // (`get_cmd_spec`). Both had a message of cash's own.
    if context.sandbox {
        return compilation_error(lines, line, ERR_SANDBOX);
    }

    line.advance(); // Skip the command character.
    line.eat_spaces(); // Skip any leading whitespace.

    if line.eol() {
        // No argument: execute the pattern space itself at runtime.
        cmd.data = CommandData::None;
        return Ok(CommandHandling::Continue);
    }

    // True after a \ at the end of a line
    let mut escaped_newline = false;

    // Skip optional \
    if line.current() == '\\' {
        line.advance();
        escaped_newline = line.eol();
    }

    // Gather the command text. Stop on a non-escaped newline. Unlike most
    // other commands, ';' does not terminate the argument. The rest of the
    // (possibly continued) line is consumed unconditionally.
    let mut text = Vec::new();
    // True once a continuation line has actually been pulled in. A dangling
    // leading backslash with no line to continue into is treated as no
    // argument at all, matching GNU sed. However once a continuation succeeds,
    // even into an empty line, we're committed to producing a (possibly empty)
    // Text argument from then on.
    let mut continued = false;
    'text_content: loop {
        if escaped_newline {
            match lines.next_line()? {
                None => {
                    break 'text_content;
                }
                Some(line_bytes) => {
                    *line = ScriptCharProvider::new(line_bytes);
                    continued = true;
                }
            }
            escaped_newline = false;
        }

        // Non-escaped newline
        if line.eol() {
            break 'text_content;
        }

        if line.current() == '\\' {
            line.advance();

            if line.eol() {
                escaped_newline = true;
                text.push(b'\n');
                continue 'text_content;
            }

            if let Some(decoded) = parse_char_escape(line) {
                push_script_char(&mut text, decoded, context.character_mode);
            } else {
                // Invalid escapes result in the escaped character.
                text.push(line.current_byte());
                line.advance();
            }
        } else {
            text.push(line.current_byte());
            line.advance();
        }
    }

    cmd.data = if text.is_empty() && !continued {
        // A dangling leading backslash with nothing left to continue into.
        // Treat this the same as no argument at all.
        CommandData::None
    } else {
        CommandData::Text(Rc::from(text))
    };
    Ok(CommandHandling::Continue)
}

// Return the specification for the command letter at the current line position
// checking for diverse errors.
fn get_verified_cmd_spec(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    n_addr: usize,
    posix: bool,
) -> UResult<CommandSpec> {
    if line.eol() {
        return compilation_error(lines, line, "missing command");
    }

    let ch = line.current();
    let cmd_spec = get_cmd_spec(lines, line, ch, posix)?;

    if n_addr > cmd_spec.n_addr {
        // GNU sed's words.
        let message = match ch {
            ':' => ": doesn't want any addresses",
            '}' => "`}' doesn't want any addresses",
            'v' => "unknown command: `v'",
            _ => "command only uses one address",
        };
        return compilation_error(lines, line, message);
    }

    Ok(cmd_spec)
}

// Look up a command addresses and handler by its command code.
fn get_cmd_spec(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    cmd_code: char,
    posix: bool,
) -> UResult<CommandSpec> {
    match cmd_code {
        '!' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_negation_command,
        }),
        '=' => Ok(CommandSpec {
            n_addr: if posix { 1 } else { 2 },
            handler: compile_empty_command,
        }),
        ':' => Ok(CommandSpec {
            n_addr: 0,
            handler: compile_label_command,
        }),
        '{' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_block_command,
        }),
        '}' => Ok(CommandSpec {
            n_addr: 0,
            handler: compile_end_group_command,
        }),
        'a' | 'i' => Ok(CommandSpec {
            n_addr: if posix { 1 } else { 2 },
            handler: compile_text_command,
        }),
        'b' | 't' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_label_command,
        }),
        'c' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_text_command,
        }),
        'd' | 'D' | 'g' | 'G' | 'h' | 'H' | 'n' | 'N' | 'p' | 'P' | 'x' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_empty_command,
        }),
        'z' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_empty_command,
        }),
        'l' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_number_command,
        }),
        // One address, also outside POSIX mode, as in GNU sed: `1,+0q` is its "command
        // only uses one address".
        'q' => Ok(CommandSpec {
            n_addr: 1,
            handler: compile_number_command,
        }),
        // Q is a GNU extension
        'Q' => Ok(CommandSpec {
            n_addr: 1,
            handler: compile_number_command,
        }),
        // e and R are GNU extensions
        'e' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_execute_command,
        }),
        'F' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_empty_command,
        }),
        'r' => Ok(CommandSpec {
            n_addr: if posix { 1 } else { 2 },
            handler: compile_read_file_command,
        }),
        'R' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_read_line_command,
        }),
        's' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_subst_command,
        }),
        'T' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_label_command,
        }),
        'W' if !posix => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_write_file_command,
        }),
        'w' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_write_file_command,
        }),
        'y' => Ok(CommandSpec {
            n_addr: 2,
            handler: compile_trans_command,
        }),
        'v' if !posix => Ok(CommandSpec {
            n_addr: 0,
            handler: compile_version_command,
        }),
        // An address before a comment: comments are read before any address.
        '#' => compilation_error(lines, line, "comments don't accept any addresses"),
        _ => compilation_error(lines, line, format!("unknown command: `{cmd_code}'")),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;
    use crate::sed::fast_io::IOChunk;
    // Return an empty line provider and a char provider for the specified str.
    fn make_providers(input: &str) -> (ScriptLineProvider, ScriptCharProvider) {
        let lines = ScriptLineProvider::new(vec![]); // Empty for tests
        let line = ScriptCharProvider::new(input);
        (lines, line)
    }

    fn make_line_provider(lines: &[&str]) -> ScriptLineProvider {
        let input = lines
            .iter()
            .map(|s| ScriptValue::StringVal((*s).to_string()))
            .collect();
        ScriptLineProvider::new(input)
    }

    fn make_char_provider(input: &str) -> ScriptCharProvider {
        ScriptCharProvider::new(input)
    }

    /// Return a default ProcessingContext for use in tests.
    pub fn ctx() -> ProcessingContext {
        ProcessingContext::default()
    }

    // get_cmd_spec
    #[test]
    fn test_lookup_empty_command() {
        let (lines, line) = make_providers("123abc");
        let cmd = get_cmd_spec(&lines, &line, 'd', false).unwrap();
        assert_eq!(cmd.n_addr, 2);
    }

    #[test]
    fn test_lookup_text_command() {
        let (lines, line) = make_providers("123abc");
        let cmd = get_cmd_spec(&lines, &line, 'a', false).unwrap();
        assert_eq!(cmd.n_addr, 2);
    }

    #[test]
    fn test_lookup_nonselect_command() {
        let (lines, line) = make_providers("123abc");
        let cmd = get_cmd_spec(&lines, &line, '!', false).unwrap();
        assert_eq!(cmd.n_addr, 2);
    }

    #[test]
    fn test_lookup_endgroup_command() {
        let (lines, line) = make_providers("123abc");
        let cmd = get_cmd_spec(&lines, &line, '}', false).unwrap();
        assert_eq!(cmd.n_addr, 0);
    }

    #[test]
    fn test_lookup_invalid_command() {
        let (lines, line) = make_providers("123abc");
        let result = get_cmd_spec(&lines, &line, 'Z', false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_command_ending_rejects_extra_characters() {
        let (lines, mut chars) = make_providers("extra");
        let err = parse_command_ending(&lines, &mut chars).unwrap_err();
        assert!(err.to_string().contains("extra characters after command"));
    }

    #[test]
    fn test_lookup_branch_commands() {
        // b, t, and T all share compile_label_command and accept 2 addresses.
        for code in ['b', 't', 'T'] {
            let (lines, line) = make_providers("123abc");
            let cmd = get_cmd_spec(&lines, &line, code, false).unwrap();
            assert_eq!(cmd.n_addr, 2, "command `{code}` should accept 2 addresses");
        }
    }

    // Utility to create a ScriptCharProvider from a &str
    fn char_provider_from(s: &str) -> ScriptCharProvider {
        ScriptCharProvider::new(s)
    }

    // compilation_error
    #[test]
    fn test_compilation_error_message_format() {
        let lines = ScriptLineProvider::with_active_state("test.sed", 42);
        let mut line = char_provider_from("whatever");
        line.advance(); // move to position 1
        line.advance(); // move to position 2
        line.advance(); // move to position 3
        line.advance(); // now at position 4

        let msg = "unexpected token";
        let result: UResult<()> = compilation_error(&lines, &line, msg);

        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("test.sed:42:5: error: unexpected token"));
    }

    #[test]
    fn test_compilation_error_with_format_message() {
        let lines = ScriptLineProvider::with_active_state("input.txt", 3);
        let line = char_provider_from("x");
        // We're at position 0

        let result: UResult<()> =
            compilation_error(&lines, &line, format!("invalid command '{}'", 'x'));

        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();

        assert_eq!(msg, "input.txt:3:1: error: invalid command 'x'");
    }

    // get_verified_cmd_spec
    #[test]
    fn test_missing_command_character() {
        let lines = ScriptLineProvider::with_active_state("test.sed", 1);
        let line = char_provider_from("");
        let result = get_verified_cmd_spec(&lines, &line, 0, ctx().posix);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("test.sed:1:0: error: missing command"));
    }

    #[test]
    fn test_invalid_command_character() {
        let lines = ScriptLineProvider::with_active_state("script.sed", 2);
        let line = char_provider_from("@");
        let result = get_verified_cmd_spec(&lines, &line, 0, ctx().posix);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("script.sed:2:1: error: unknown command: `@'"));
    }

    #[test]
    fn test_too_many_addresses() {
        let lines = ScriptLineProvider::with_active_state("input.sed", 3);
        let line = char_provider_from("q"); // q takes one address
        let result = get_verified_cmd_spec(&lines, &line, 2, true);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("input.sed:3:1: error: command only uses one address"));
    }

    #[test]
    fn test_valid_command_spec() {
        let lines = ScriptLineProvider::with_active_state("input.sed", 4);
        let line = char_provider_from("a"); // valid command
        let result = get_verified_cmd_spec(&lines, &line, 2, ctx().posix);
        assert!(result.is_ok());
        let spec = result.unwrap();
        assert_eq!(spec.n_addr, 2);
    }

    #[test]
    fn test_invalid_address_range_posix() {
        let lines = ScriptLineProvider::with_active_state("input.sed", 1);
        let line = char_provider_from("i"); // valid command
        let result = get_verified_cmd_spec(&lines, &line, 2, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("input.sed:1:1: error: command only uses one address"));
    }

    // parse_number
    #[test]
    fn test_parse_number_basic() {
        let (lines, mut chars) = make_providers("123abc");
        assert_eq!(parse_number(&lines, &mut chars, true).unwrap(), Some(123));
        assert_eq!(chars.current(), 'a'); // Should stop at first non-digit
    }

    #[test]
    fn test_parse_optional_number_missing() {
        let (lines, mut chars) = make_providers(" ;");
        assert_eq!(parse_number(&lines, &mut chars, false).unwrap(), None);
    }

    #[test]
    fn test_parse_number_invalid() {
        let (lines, mut chars) = make_providers("537654897563495734653453434534534534545");
        let err = parse_number(&lines, &mut chars, true).unwrap_err();
        assert!(err.to_string().contains("invalid number"));
    }

    #[test]
    fn test_parse_required_number_missing() {
        let (lines, mut chars) = make_providers("");
        let err = parse_number(&lines, &mut chars, true).unwrap_err();
        assert!(err.to_string().contains("number expected"));
    }

    // compile_re
    fn dummy_providers() -> (ScriptLineProvider, ScriptCharProvider) {
        make_providers("dummy input")
    }

    #[test]
    fn test_compile_re_basic() {
        let (lines, _) = dummy_providers();
        let regex = compile_regex(&lines, 1, "abc", &ctx(), false, false)
            .unwrap()
            .expect("regex should be present");
        assert!(regex.is_match(&mut IOChunk::new_from_str("abc")).unwrap());
        assert!(!regex.is_match(&mut IOChunk::new_from_str("ABC")).unwrap());
    }

    #[test]
    fn test_compile_re_extended() {
        let (lines, _) = make_providers("acaa\nbbb\nccc");
        let mut ctx = ctx();
        ctx.regex_extended = true;
        let regex = compile_regex(&lines, 1, "cc{0,}", &ctx, false, false)
            .unwrap()
            .expect("regex should be present");
        assert!(
            regex
                .is_match(&mut IOChunk::new_from_str("acaa\nccc"))
                .unwrap()
        );
    }

    #[test]
    fn test_compile_re_case_insensitive() {
        let (lines, _) = dummy_providers();
        let regex = compile_regex(&lines, 1, "abc", &ctx(), true, false)
            .unwrap()
            .expect("regex should be present");
        assert!(regex.is_match(&mut IOChunk::new_from_str("abc")).unwrap());
        assert!(regex.is_match(&mut IOChunk::new_from_str("ABC")).unwrap());
        assert!(regex.is_match(&mut IOChunk::new_from_str("AbC")).unwrap());
    }

    #[test]
    fn test_compile_re_invalid() {
        let (lines, _) = dummy_providers();
        let result = compile_regex(&lines, 1, "a[d", &ctx(), false, false);
        assert!(result.is_err()); // Should fail due to open bracketed expression
    }

    #[test]
    fn test_compile_re_multiline_start() {
        let (lines, _) = dummy_providers();
        let regex = compile_regex(&lines, 1, "^bar", &ctx(), false, true)
            .unwrap()
            .expect("regex should be present");
        assert!(
            regex
                .is_match(&mut IOChunk::new_from_str("foo\nbar"))
                .unwrap()
        );
    }

    #[test]
    fn test_compile_re_multiline_end() {
        let (lines, _) = dummy_providers();
        let regex = compile_regex(&lines, 1, "foo$", &ctx(), false, true)
            .unwrap()
            .expect("regex should be present");
        assert!(
            regex
                .is_match(&mut IOChunk::new_from_str("foo\nbar"))
                .unwrap()
        );
    }

    // compile_address
    #[test]
    fn test_compile_addr_line_number() {
        let (lines, mut chars) = make_providers("42");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();
        assert!(matches!(addr, Address::Line(42)));
    }

    #[test]
    fn test_compile_addr_relative_line() {
        let (lines, mut chars) = make_providers("+7");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();
        assert!(matches!(addr, Address::RelLine(7)));
    }

    #[test]
    fn test_compile_addr_last_line() {
        let (lines, mut chars) = make_providers("$");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();
        assert!(matches!(addr, Address::Last));
    }

    #[test]
    fn test_compile_addr_regex() {
        let (lines, mut chars) = make_providers("/hello/");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();

        let Address::Re(Some(re)) = addr else {
            panic!("expected Address::Re(Some(_))");
        };

        assert!(re.is_match(&mut IOChunk::new_from_str("hello")).unwrap());
    }

    #[test]
    fn test_compile_addr_regex_backref_match() {
        let (lines, mut chars) = make_providers(r"/he\(.\)\1o/");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();

        match addr {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("hello")).unwrap());
            }
            _ => panic!("expected Address::Re(Some(_))"),
        }
    }

    #[test]
    fn test_compile_addr_regex_backref_no_match() {
        let (lines, mut chars) = make_providers(r"/he\(.\)\1o/");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();

        match addr {
            Address::Re(Some(re)) => {
                assert!(!re.is_match(&mut IOChunk::new_from_str("helio")).unwrap());
            }
            _ => panic!("expected Address::Re(Some(_))"),
        }
    }

    #[test]
    fn test_compile_addr_regex_other_delimiter() {
        let (lines, mut chars) = make_providers("\\#hello#");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();

        match addr {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("hello")).unwrap());
            }
            _ => panic!("expected Address::Re(Some(_))"),
        }
    }

    #[test]
    fn test_compile_addr_regex_with_modifier() {
        let (lines, mut chars) = make_providers("/hello/I");
        let addr = compile_address(&lines, &mut chars, &ctx()).unwrap();

        match addr {
            Address::Re(Some(re)) => {
                // Case-insensitive
                assert!(re.is_match(&mut IOChunk::new_from_str("HELLO")).unwrap());
            }
            _ => panic!("expected Address::Re(Some(_))"),
        }
    }

    // compile_address_range
    #[test]
    fn test_compile_single_line_address() {
        let (lines, mut chars) = make_providers("42");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 1);
        assert!(matches!(cmd.borrow().addr1, Some(Address::Line(42))));
    }

    #[test]
    fn test_compile_relative_address_range() {
        let (lines, mut chars) = make_providers("2,+3");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 2);

        assert!(matches!(cmd.borrow().addr1, Some(Address::Line(2))));
        assert!(matches!(cmd.borrow().addr2, Some(Address::RelLine(3))));
    }

    #[test]
    fn test_compile_step_match_address() {
        let (lines, mut chars) = make_providers("0~2");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 1);
        assert!(matches!(
            cmd.borrow().addr1,
            Some(Address::Step { first: 0, step: 2 })
        ));
        assert!(cmd.borrow().addr2.is_none());
    }

    // `first~step` is one address, as in GNU sed, so it can start a range; it was read as
    // a second address, and `1~3,5p` was refused (TODO.md 14.6).
    #[test]
    fn test_compile_step_match_starts_a_range() {
        let (lines, mut chars) = make_providers("1 ~ 3,5p");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 2);
        assert!(matches!(
            cmd.borrow().addr1,
            Some(Address::Step { first: 1, step: 3 })
        ));
        assert!(matches!(cmd.borrow().addr2, Some(Address::Line(5))));
        assert_eq!(chars.current(), 'p');
    }

    #[test]
    fn test_compile_step_end_address() {
        let (lines, mut chars) = make_providers("1,~10");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 2);
        assert!(matches!(cmd.borrow().addr1, Some(Address::Line(1))));
        assert!(matches!(cmd.borrow().addr2, Some(Address::StepEnd(10))));
    }

    // As in GNU sed, a step that is not a number is a step of 0, so `1~` is line 1 and
    // what follows is left to the command: `1~/x/` is the command `/`.
    #[test]
    fn test_compile_step_without_a_number_is_zero() {
        let (lines, mut chars) = make_providers("1~/x/");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();
        assert_eq!(n_addr, 1);
        assert!(matches!(cmd.borrow().addr1, Some(Address::Line(1))));
        assert!(cmd.borrow().addr2.is_none());
        assert_eq!(chars.current(), '/');

        for (script, step) in [("2,~p", 0), ("2,~ 3p", 3)] {
            let (lines, mut chars) = make_providers(script);
            let mut cmd = Rc::new(RefCell::new(Command::default()));
            let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();
            assert_eq!(n_addr, 2, "{script}");
            assert!(matches!(cmd.borrow().addr2, Some(Address::StepEnd(s)) if s == step));
            assert_eq!(chars.current(), 'p');
        }

        for (script, count) in [("2,+p", 0), ("2,+ 1p", 1)] {
            let (lines, mut chars) = make_providers(script);
            let mut cmd = Rc::new(RefCell::new(Command::default()));
            let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();
            assert_eq!(n_addr, 2, "{script}");
            assert!(matches!(cmd.borrow().addr2, Some(Address::RelLine(n)) if n == count));
            assert_eq!(chars.current(), 'p');
        }

        // At the end of the line, where there was no current character to look at.
        let (lines, mut chars) = make_providers("1~");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        assert_eq!(
            compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap(),
            1
        );
    }

    #[test]
    fn test_compile_last_address() {
        let (lines, mut chars) = make_providers("$");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 1);
        assert!(matches!(cmd.borrow().addr1, Some(Address::Last)));
    }

    #[test]
    fn test_compile_absolute_address_range() {
        let (lines, mut chars) = make_providers("5,10");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 2);
        assert!(matches!(cmd.borrow().addr1, Some(Address::Line(5))));
        assert!(matches!(cmd.borrow().addr2, Some(Address::Line(10))));
    }

    #[test]
    fn test_compile_regex_address() {
        let (lines, mut chars) = make_providers("/foo/");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 1);

        match cmd.borrow().addr1.as_ref().unwrap() {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("foo")).unwrap());
                assert!(!re.is_match(&mut IOChunk::new_from_str("bar")).unwrap());
            }
            _ => panic!("expected regex address"),
        }
    }

    #[test]
    fn test_compile_regex_address_range_other_delimiter() {
        let (lines, mut chars) = make_providers("\\#foo# , \\|bar|");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 2);

        match cmd.borrow().addr1.as_ref().unwrap() {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("foo")).unwrap());
                assert!(!re.is_match(&mut IOChunk::new_from_str("bar")).unwrap());
            }
            _ => panic!("expected regex address"),
        }

        match cmd.borrow().addr2.as_ref().unwrap() {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("bar")).unwrap());
                assert!(!re.is_match(&mut IOChunk::new_from_str("foo")).unwrap());
            }
            _ => panic!("expected regex address"),
        }
    }

    #[test]
    fn test_compile_regex_with_modifier() {
        let (lines, mut chars) = make_providers("/foo/I");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

        assert_eq!(n_addr, 1);

        match cmd.borrow().addr1.as_ref().unwrap() {
            Address::Re(Some(re)) => {
                assert!(re.is_match(&mut IOChunk::new_from_str("FOO")).unwrap());
                assert!(re.is_match(&mut IOChunk::new_from_str("foo")).unwrap());
            }
            _ => panic!("expected regex address"),
        }
    }

    #[test]
    fn test_compile_address_range_error_propagation() {
        let (lines, mut chars) = make_providers("1,/abc");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let result = compile_address_range(&lines, &mut chars, &mut cmd, &ctx());

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains(ERR_UNTERMINATED_ADDRESS_REGEX));
    }

    // compile_sequence
    fn empty_line() -> ScriptCharProvider {
        ScriptCharProvider::new("")
    }

    #[test]
    fn test_zero_addr_r_accepted() {
        for input in ["0r", "0  r"] {
            let (lines, mut chars) = make_providers(input);
            let mut cmd = Rc::new(RefCell::new(Command::default()));
            let n_addr = compile_address_range(&lines, &mut chars, &mut cmd, &ctx()).unwrap();

            assert_eq!(n_addr, 1);
            assert!(matches!(cmd.borrow().addr1, Some(Address::Line(0))));
            assert_eq!(chars.current(), 'r');
        }
    }

    // Zero-address with no commands
    #[test]
    fn test_zero_addr_no_commands() {
        let (lines, mut chars) = make_providers("0");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let result = compile_address_range(&lines, &mut chars, &mut cmd, &ctx());

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains(ERR_ADDRESS_0_USAGE)
        );
    }

    // Zero-address with a command other than 'r' must still be rejected.
    #[test]
    fn test_zero_addr_non_r_rejected() {
        let (lines, mut chars) = make_providers("0p");
        let mut cmd = Rc::new(RefCell::new(Command::default()));
        let result = compile_address_range(&lines, &mut chars, &mut cmd, &ctx());

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains(ERR_ADDRESS_0_USAGE)
        );
    }

    #[test]
    fn test_compile_sequence_empty_input() {
        let mut provider = make_line_provider(&[]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_compile_sequence_comment_only() {
        let mut provider = make_line_provider(&["# comment", "   ", ";;"]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_compile_sequence_single_command() {
        let mut provider = make_line_provider(&["42q"]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        let binding = result.unwrap();
        let cmd = binding.borrow();

        assert_eq!(cmd.code, 'q');
        assert!(!cmd.non_select);

        assert!(matches!(cmd.addr1, Some(Address::Line(42))));
        assert!(cmd.next.is_none());
    }

    #[test]
    fn test_compile_sequence_non_selected_single_command() {
        let mut provider = make_line_provider(&["42!p"]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        let binding = result.unwrap();
        let cmd = binding.borrow();

        assert_eq!(cmd.code, 'p');
        assert!(cmd.non_select);

        assert!(matches!(cmd.addr1, Some(Address::Line(42))));
        assert!(cmd.next.is_none());
    }

    #[test]
    fn test_compile_sequence_multiple_lines() {
        let mut provider = make_line_provider(&["1q", "2d"]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        let binding = result.unwrap();
        let first = binding.borrow();

        assert_eq!(first.code, 'q');
        let binding = first.next.clone().unwrap();
        let second = binding.borrow();
        assert_eq!(second.code, 'd');
        assert!(second.next.is_none());
    }

    #[test]
    fn test_compile_sequence_single_line_multiple_commands() {
        let mut provider = make_line_provider(&["1q;2d"]);
        let mut opts = ctx();

        let result = compile_sequence(&mut provider, &mut empty_line(), &mut opts).unwrap();
        let binding = result.unwrap();
        let first = binding.borrow();

        assert_eq!(first.code, 'q');
        let binding = first.next.clone().unwrap();
        let second = binding.borrow();
        assert_eq!(second.code, 'd');
        assert!(second.next.is_none());
    }

    // compile
    #[test]
    fn test_compile_single_command() {
        let scripts = vec![ScriptValue::StringVal("1q".to_string())];
        let mut opts = ProcessingContext::default();

        let result = compile(scripts, &mut opts).unwrap();
        let binding = result.unwrap();
        let cmd = binding.borrow();

        assert_eq!(cmd.code, 'q');

        assert!(matches!(cmd.addr1, Some(Address::Line(1))));

        assert_eq!(cmd.location.line_number, 1);
        assert_eq!(cmd.location.column_number, 1);
        assert_eq!(cmd.location.input_name.as_ref(), "<script argument 1>");

        assert!(cmd.next.is_none());
    }

    #[test]
    fn test_compile_two_commands() {
        let scripts = vec![ScriptValue::StringVal("l;q".to_string())];
        let mut opts = ProcessingContext::default();

        let result = compile(scripts, &mut opts).unwrap();
        let binding = result.unwrap();
        let cmd = binding.borrow();

        assert_eq!(cmd.code, 'l');
        assert_eq!(cmd.location.line_number, 1);
        assert_eq!(cmd.location.column_number, 1);
        assert_eq!(cmd.location.input_name.as_ref(), "<script argument 1>");

        let binding2 = cmd.next.clone().unwrap();
        let cmd2 = binding2.borrow();
        assert_eq!(cmd2.code, 'q');
        assert_eq!(cmd2.location.line_number, 1);
        assert_eq!(cmd2.location.column_number, 3);
        assert_eq!(cmd2.location.input_name.as_ref(), "<script argument 1>");

        assert!(cmd2.next.is_none());
    }

    // compile_replacement

    /// Compile a regular expression replacement string in UTF-8 mode.
    fn compile_replacement_utf8(
        lines: &mut ScriptLineProvider,
        line: &mut ScriptCharProvider,
    ) -> UResult<ReplacementTemplate> {
        compile_replacement(lines, line, CharacterMode::Utf8, true)
    }

    #[test]
    fn test_compile_replacement_literal() {
        let (mut lines, mut chars) = make_providers("/hello/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"hello"));
    }

    #[test]
    fn test_compile_replacement_escaped_delimiter() {
        let (mut lines, mut chars) = make_providers(r"/hell\/o/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"hell/o"));
    }

    #[test]
    fn test_compile_replacement_backrefs_and_literal() {
        let (mut lines, mut chars) = make_providers("/prefix \\1 and \\2/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 4);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"prefix "));
        assert!(matches!(&template.parts[1], ReplacementPart::Group(1)));
        assert!(matches!(&template.parts[2], ReplacementPart::Literal(s) if s == b" and "));
        assert!(matches!(&template.parts[3], ReplacementPart::Group(2)));
    }

    #[test]
    fn test_compile_replacement_whole_match() {
        let (mut lines, mut chars) = make_providers("/The match was: &/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 2);
        assert!(
            matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"The match was: ")
        );
        assert!(matches!(&template.parts[1], ReplacementPart::WholeMatch));
    }

    #[test]
    fn test_compile_replacement_whole_match_synonym() {
        let (mut lines, mut chars) = make_providers(r"/The match was: \0/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 2);
        assert!(
            matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"The match was: ")
        );
        assert!(matches!(&template.parts[1], ReplacementPart::WholeMatch));
    }

    #[test]
    fn test_compile_replacement_ampersand() {
        let (mut lines, mut chars) = make_providers("/Simon \\& Garfunkel/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(
            matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"Simon & Garfunkel")
        );
    }

    #[test]
    fn test_compile_replacement_escape_sequences() {
        let (mut lines, mut chars) = make_providers("/line\\nnewline\\tend/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(
            &template.parts[0],
            ReplacementPart::Literal(s) if s == b"line\nnewline\tend"
        ));
    }

    #[test]
    fn test_compile_replacement_escape_byte_mode() {
        let (mut lines, mut chars) = make_providers("/\\xE9/");
        let template =
            compile_replacement(&mut lines, &mut chars, CharacterMode::Byte, true).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"\xE9"));
    }

    #[test]
    fn test_compile_replacement_escape_utf8_mode() {
        let (mut lines, mut chars) = make_providers("/\\xE9/");
        let template =
            compile_replacement(&mut lines, &mut chars, CharacterMode::Utf8, true).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(
            &template.parts[0],
            ReplacementPart::Literal(s) if s == "é".as_bytes()
        ));
    }

    #[test]
    fn test_compile_replacement_line_continuation() {
        let script = vec![
            ScriptValue::StringVal("/first line\\".to_string()),
            ScriptValue::StringVal(" continued/".to_string()),
        ];
        let mut provider = ScriptLineProvider::new(script);
        let first_line = provider.next_line().unwrap().unwrap();
        let mut chars = ScriptCharProvider::new(first_line);

        let template = compile_replacement_utf8(&mut provider, &mut chars).unwrap();
        assert_eq!(template.parts.len(), 1);
        assert!(matches!(
            &template.parts[0],
            ReplacementPart::Literal(s) if s == b"first line\n continued"
        ));
    }

    #[test]
    fn test_compile_replacement_preserves_invalid_utf8_script_byte() {
        let mut chars = ScriptCharProvider::new(b"/\xC2/");
        let mut lines = ScriptLineProvider::new(vec![]);

        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"\xC2"));
    }

    #[test]
    fn test_compile_replacement_unknown_escape_is_the_character() {
        let (mut lines, mut chars) = make_providers(r"/a\q/");
        let template = compile_replacement_utf8(&mut lines, &mut chars).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert!(matches!(&template.parts[0], ReplacementPart::Literal(s) if s == b"aq"));
    }

    #[test]
    fn test_compile_replacement_eof_after_backslash() {
        let (mut lines, mut chars) = make_providers(r"/abc\");
        let err = compile_replacement_utf8(&mut lines, &mut chars).unwrap_err();

        assert!(err.to_string().contains(ERR_UNTERMINATED_S));
    }

    #[test]
    fn test_compile_replacement_unescaped_newline() {
        let (mut lines, mut chars) = make_providers("/abc\n/");
        let err = compile_replacement_utf8(&mut lines, &mut chars).unwrap_err();

        assert!(
            err.to_string()
                .contains("unescaped newline inside substitute replacement")
        );
    }

    #[test]
    fn test_compile_replacement_unterminated() {
        let (mut lines, mut chars) = make_providers("/abc");
        let err = compile_replacement_utf8(&mut lines, &mut chars).unwrap_err();

        assert!(err.to_string().contains(ERR_UNTERMINATED_S));
    }

    // compile_subst_flags
    #[test]
    fn test_compile_subst_flag_g() {
        let (lines, mut chars) = make_providers("g");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.global);
        assert_eq!(subst.occurrence, 1);
    }

    #[test]
    fn test_compile_subst_flag_p() {
        let (lines, mut chars) = make_providers("p");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.print_flag);
    }

    #[test]
    fn test_compile_subst_flag_uppercase_i() {
        let (lines, mut chars) = make_providers("I");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.ignore_case);
    }

    #[test]
    fn test_compile_subst_flag_i_lowercase() {
        let (lines, mut chars) = make_providers("i");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.ignore_case);
    }

    #[test]
    fn test_compile_subst_flag_uppercase_m() {
        let (lines, mut chars) = make_providers("M");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.multiline);
    }

    #[test]
    fn test_compile_subst_flag_m_lowercase() {
        let (lines, mut chars) = make_providers("m");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.multiline);
    }

    #[test]
    fn test_compile_subst_flag_number() {
        let (lines, mut chars) = make_providers("3");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert_eq!(subst.occurrence, 3);
    }

    #[test]
    fn test_compile_subst_flag_g_and_number_combines() {
        let (lines, mut chars) = make_providers("g3");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.global);
        assert_eq!(subst.occurrence, 3);
    }

    #[test]
    fn test_compile_subst_flag_number_and_g_combines() {
        let (lines, mut chars) = make_providers("2g");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.global);
        assert_eq!(subst.occurrence, 2);
    }

    #[test]
    fn test_compile_subst_flag_duplicate_g_should_fail() {
        let (lines, mut chars) = make_providers("gg");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap_err();
        assert!(
            err.to_string()
                .contains("multiple `g' options to `s' command")
        );
    }

    #[test]
    fn test_compile_subst_flag_duplicate_number_should_fail() {
        let (lines, mut chars) = make_providers("2p3");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap_err();
        assert!(
            err.to_string()
                .contains("multiple number options to `s' command")
        );
    }

    #[test]
    fn test_compile_subst_flag_w_missing_filename() {
        let (lines, mut chars) = make_providers("w ");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing filename in r/R/w/W commands")
        );
    }

    #[test]
    fn test_compile_subst_flag_w_with_filename() {
        let tmp_dir = tempfile::tempdir().expect("failed to create tmp folder");
        let out = tmp_dir.path().join("out.txt");
        let (lines, mut chars) = make_providers(&format!("w {}", out.display()));
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert_eq!(
            subst.write_file.as_ref().map(|w| w.borrow().path.clone()),
            Some(out)
        );
    }

    #[test]
    fn test_compile_subst_flag_w_rejected_under_sandbox() {
        let (lines, mut chars) = make_providers("w out.txt");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, true).unwrap_err();
        assert!(err.to_string().contains(ERR_SANDBOX));
    }

    #[test]
    fn test_compile_subst_flag_e() {
        let (lines, mut chars) = make_providers("e");
        let mut subst = Substitution::default();

        compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap();
        assert!(subst.execute);
    }

    #[test]
    fn test_compile_subst_flag_e_rejected_under_posix() {
        let (lines, mut chars) = make_providers("e");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, true, false).unwrap_err();
        assert!(err.to_string().contains(ERR_UNKNOWN_OPTION_TO_S));
    }

    #[test]
    fn test_compile_subst_flag_e_rejected_under_sandbox() {
        let (lines, mut chars) = make_providers("e");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, true).unwrap_err();
        assert!(err.to_string().contains(ERR_SANDBOX));
    }

    #[test]
    fn test_compile_subst_flag_invalid_flag() {
        let (lines, mut chars) = make_providers("z");
        let mut subst = Substitution::default();

        let err = compile_subst_flags(&lines, &mut chars, &mut subst, false, false).unwrap_err();
        assert!(err.to_string().contains(ERR_UNKNOWN_OPTION_TO_S));
    }

    // compile_subst_command
    #[test]
    fn test_compile_subst_invalid_delimiter_backslash() {
        let (mut lines, mut chars) = make_providers("s\\foo\\bar\\");
        let mut cmd = Command::default();
        let mut context = ctx();

        let err =
            compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(
            err.to_string()
                .contains("substitute pattern cannot be delimited")
        );
    }

    #[test]
    fn test_compile_subst_extra_characters_at_end() {
        let (mut lines, mut chars) = make_providers("s/foo/bar/x");
        let mut cmd = Command::default();
        let mut context = ctx();

        let err =
            compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(err.to_string().contains(ERR_UNKNOWN_OPTION_TO_S));
    }

    #[test]
    fn test_compile_subst_semicolon_indicates_continue() {
        let (mut lines, mut chars) = make_providers("s/foo/bar/;");
        let mut cmd = Command::default();
        let mut context = ctx();

        compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();

        if let CommandData::Substitution(subst) = &cmd.data {
            assert_eq!(subst.replacement.parts.len(), 1);
        } else {
            panic!("Expected CommandData::Substitution");
        }
    }

    #[test]
    fn test_compile_subst_sets_command_data() {
        let (mut lines, mut chars) = make_providers("s/foo/bar/");
        let mut cmd = Command::default();
        let mut context = ctx();

        compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Substitution(subst) => {
                assert_eq!(subst.replacement.parts.len(), 1);
                assert!(
                    matches!(&subst.replacement.parts[0], ReplacementPart::Literal(s) if s == b"bar")
                );
            }
            _ => panic!("Expected CommandData::Substitution"),
        }
    }

    #[test]
    fn test_compile_subst_invalid_group_reference() {
        let (mut lines, mut chars) = make_providers(r"s/f(o)o/\2/");
        let mut cmd = Command::default();
        let mut context = ctx();

        let err =
            compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(err.to_string().contains("invalid reference \\2"));
    }

    #[test]
    fn test_compile_subst_empty_re_rejects_modifiers() {
        let (mut lines, mut chars) = make_providers("s//x/I");
        let mut cmd = Command::default();
        let mut context = ctx();

        let err =
            compile_subst_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot specify modifiers on empty regexp")
        );
    }

    #[test]
    fn test_compile_trans_command_sets_command_data() {
        let (mut lines, mut chars) = make_providers("y/ab/xy/");
        let mut cmd = Command::default();
        let mut context = ProcessingContext {
            character_mode: CharacterMode::Byte,
            ..ctx()
        };

        compile_trans_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Transliteration(trans) => {
                assert_eq!(trans.lookup_byte(b'a'), b'x');
                assert_eq!(trans.lookup_byte(b'b'), b'y');
                assert_eq!(trans.lookup_byte(b'c'), b'c');
            }
            _ => panic!("Expected CommandData::Transliteration"),
        }
    }

    // bre_to_ere
    fn bre_to_ere_string(pattern: &str) -> String {
        String::from_utf8(bre_to_ere(pattern.as_bytes(), false)).unwrap()
    }

    #[test]
    fn test_bre_group_translation() {
        assert_eq!(bre_to_ere_string(r"\(a\?b\+c\|\)"), "(a?b+c|)");
        assert_eq!(bre_to_ere_string(r"a\(b\)c"), "a(b)c");
    }

    #[test]
    fn test_bre_brace_quantifier_translation() {
        assert_eq!(bre_to_ere_string(r"\{1,4\}"), "{1,4}");
    }

    #[test]
    fn test_ere_metacharacters_escaped() {
        assert_eq!(bre_to_ere_string(r"a+b?c{1}|(d)"), r"a\+b\?c\{1\}\|\(d\)");
    }

    #[test]
    fn test_literal_backslashes_preserved() {
        assert_eq!(bre_to_ere_string(r"foo\\bar"), r"foo\\bar");
        assert_eq!(bre_to_ere_string(r"\."), r"\.");
    }

    #[test]
    fn test_character_classes_unchanged() {
        assert_eq!(bre_to_ere_string(r"[a-z]"), "[a-z]");
        assert_eq!(bre_to_ere_string(r"[^0-9]"), "[^0-9]");
    }

    #[test]
    fn test_anchors_and_dot_and_star() {
        assert_eq!(bre_to_ere_string(r"^a.*b$"), "^a.*b$");
    }

    #[test]
    fn test_trailing_backslash_is_preserved() {
        assert_eq!(bre_to_ere_string(r"abc\"), r"abc\");
    }

    #[test]
    fn test_caret_escaped_in_middle() {
        assert_eq!(bre_to_ere_string(r"^a^[^x]c"), r"^a\^[^x]c");
    }

    #[test]
    fn test_dollar_escaped_in_middle() {
        assert_eq!(bre_to_ere_string(r"a$c$"), r"a\$c$");
    }

    #[test]
    fn test_bre_back_reference() {
        assert_eq!(bre_to_ere_string(r"\(.\)\1\(.\)\2"), r"(.)(?:\1)(.)(?:\2)");
    }

    // patch_block_endings

    // Create a command with the specified code.
    fn command_with_code(code: char) -> Rc<RefCell<Command>> {
        Rc::new(RefCell::new(Command {
            code,
            ..Default::default()
        }))
    }

    // Link the vector of passed commands into a list, returning head.
    fn link_commands(cmds: Vec<Rc<RefCell<Command>>>) -> Option<Rc<RefCell<Command>>> {
        for i in 0..cmds.len().saturating_sub(1) {
            cmds[i].borrow_mut().next = Some(cmds[i + 1].clone());
        }
        cmds.first().cloned()
    }

    // Return the command codes along the passed linked list.
    fn collect_codes(mut head: Option<Rc<RefCell<Command>>>) -> Vec<char> {
        let mut result = Vec::new();
        while let Some(cmd) = head {
            let cmd_ref = cmd.borrow();
            result.push(cmd_ref.code);
            head = cmd_ref.next.clone();
        }
        result
    }

    #[test]
    fn test_flat_chain() {
        let a = command_with_code('a');
        let b = command_with_code('b');
        let head = link_commands(vec![a, b]);

        patch_block_endings(head.clone());

        assert_eq!(collect_codes(head), vec!['a', 'b']);
    }

    #[test]
    fn test_simple_block_relinks_tail() {
        // a ; { x ; y ; } b
        let a = command_with_code('a');
        let block = command_with_code('{');
        let x = command_with_code('x');
        let y = command_with_code('y');
        let b = command_with_code('b');

        let head = link_commands(vec![a.clone(), block.clone(), b]);
        let sub_head = link_commands(vec![x, y]);
        block.borrow_mut().data = CommandData::BranchTarget(sub_head.clone());

        patch_block_endings(head);

        // Expect x -> y -> b
        assert_eq!(collect_codes(sub_head), vec!['x', 'y', 'b']);
        // Expect a -> { -> b still valid
        assert_eq!(collect_codes(Some(a)), vec!['a', '{', 'b']);
    }

    #[test]
    fn test_empty_block_no_panic() {
        let a = command_with_code('a');
        a.borrow_mut().data = CommandData::BranchTarget(None);

        patch_block_endings(Some(a.clone()));

        assert_eq!(collect_codes(Some(a)), vec!['a']);
    }

    #[test]
    fn test_nested_blocks() {
        // a
        // {
        //   m
        //   {
        //     x
        //     y
        //   }
        //   n
        // }
        // b
        let a = command_with_code('a');
        let b = command_with_code('b');
        let x = command_with_code('x');
        let y = command_with_code('y');
        let m = command_with_code('m');
        let n = command_with_code('n');
        let outer_block = command_with_code('{');
        let inner_block = command_with_code('{');

        let head = link_commands(vec![a, outer_block.clone(), b]);
        let outer = link_commands(vec![m, inner_block.clone(), n]);
        let inner = link_commands(vec![x, y]);
        outer_block.borrow_mut().data = CommandData::BranchTarget(outer.clone());
        inner_block.borrow_mut().data = CommandData::BranchTarget(inner.clone());

        patch_block_endings(head.clone());

        assert_eq!(collect_codes(head), vec!['a', '{', 'b']);
        assert_eq!(collect_codes(inner), vec!['x', 'y', 'n', 'b']);
        assert_eq!(collect_codes(outer), vec!['m', '{', 'n', 'b']);
    }

    #[test]
    fn test_empty_nested_blocks() {
        // a
        // {
        //   {
        //     x
        //   }
        // }
        // b
        let a = command_with_code('a');
        let b = command_with_code('b');
        let x = command_with_code('x');
        let outer_block = command_with_code('{');
        let inner_block = command_with_code('{');

        let head = link_commands(vec![a, outer_block.clone(), b]);
        let outer = link_commands(vec![inner_block.clone()]);
        let inner = link_commands(vec![x]);
        outer_block.borrow_mut().data = CommandData::BranchTarget(outer.clone());
        inner_block.borrow_mut().data = CommandData::BranchTarget(inner.clone());

        patch_block_endings(head.clone());

        assert_eq!(collect_codes(head), vec!['a', '{', 'b']);
        assert_eq!(collect_codes(outer), vec!['{', 'b']);
        assert_eq!(collect_codes(inner), vec!['x', 'b']);
    }

    // compile_read_file_command
    #[test]
    fn test_compile_read_file_command_rejected_under_sandbox() {
        let (mut lines, mut chars) = make_providers("r input.txt");
        let mut cmd = Command::default();
        let mut context = ctx();
        context.sandbox = true;

        let err =
            compile_read_file_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(err.to_string().contains(ERR_SANDBOX));
    }

    // compile_write_file_command
    #[test]
    fn test_compile_write_file_command_rejected_under_sandbox() {
        let (mut lines, mut chars) = make_providers("w out.txt");
        let mut cmd = Command::default();
        let mut context = ctx();
        context.sandbox = true;

        let err =
            compile_write_file_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(err.to_string().contains(ERR_SANDBOX));
    }

    // compile_label_command
    #[test]
    fn test_compile_label_command() {
        let (mut lines, mut chars) = make_providers(": foo");
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_label_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Label(label) => {
                let name = label.clone().unwrap();
                assert_eq!(name, "foo");
            }
            _ => panic!("Expected CommandData::Label"),
        }
    }

    #[test]
    fn test_compile_missing_label_command() {
        let (mut lines, mut chars) = make_providers(": ;");
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        cmd.code = ':';
        let err =
            compile_label_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap_err();
        assert!(err.to_string().contains("\":\" lacks a label"));
    }

    #[test]
    fn test_compile_empty_label_command() {
        let (mut lines, mut chars) = make_providers("b ;");
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        cmd.code = 'b';
        compile_label_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Label(label) => {
                assert!(label.is_none());
            }
            _ => panic!("Expected CommandData::Label(None)"),
        }
    }

    // populate_label_map
    fn command_with_data(data: CommandData) -> Rc<RefCell<Command>> {
        Rc::new(RefCell::new(Command {
            data,
            ..Default::default()
        }))
    }

    #[test]
    fn test_single_label() {
        let cmd = command_with_data(CommandData::Label(Some("start".to_string())));
        cmd.borrow_mut().code = ':';
        let mut context = ProcessingContext::default();

        populate_label_map(Some(cmd.clone()), &mut context).unwrap();

        assert_eq!(context.label_to_command_map.len(), 1);
        assert!(context.label_to_command_map.contains_key("start"));
        assert!(Rc::ptr_eq(&context.label_to_command_map["start"], &cmd));
    }

    #[test]
    fn test_label_inside_block() {
        let nested = command_with_data(CommandData::Label(Some("inside".to_string())));
        nested.borrow_mut().code = ':';
        let block = command_with_data(CommandData::BranchTarget(Some(nested.clone())));
        let mut context = ProcessingContext::default();

        populate_label_map(Some(block), &mut context).unwrap();

        assert_eq!(context.label_to_command_map.len(), 1);
        assert!(context.label_to_command_map.contains_key("inside"));
        assert!(Rc::ptr_eq(&context.label_to_command_map["inside"], &nested));
    }

    #[test]
    fn test_multiple_labels() {
        let a = command_with_data(CommandData::Label(Some("a".to_string())));
        a.borrow_mut().code = ':';
        let b = command_with_data(CommandData::Label(Some("b".to_string())));
        b.borrow_mut().code = ':';
        let head = link_commands(vec![a, b]);

        let mut context = ProcessingContext::default();
        populate_label_map(head, &mut context).unwrap();

        assert_eq!(context.label_to_command_map.len(), 2);
        assert!(context.label_to_command_map.contains_key("a"));
        assert!(context.label_to_command_map.contains_key("b"));
    }

    #[test]
    fn test_no_labels() {
        let a = command_with_data(CommandData::None);
        let b = command_with_data(CommandData::None);
        let head = link_commands(vec![a, b]);

        let mut context = ProcessingContext::default();
        populate_label_map(head, &mut context).unwrap();

        assert_eq!(context.label_to_command_map.len(), 0);
    }

    #[test]
    fn test_label_none_is_ignored() {
        let cmd = command_with_data(CommandData::Label(None));
        let mut context = ProcessingContext::default();

        populate_label_map(Some(cmd), &mut context).unwrap();

        // The map should remain empty since the label is None
        assert_eq!(context.label_to_command_map.len(), 0);
    }

    #[test]
    fn test_duplicate_label_gives_error() {
        let a1 = command_with_data(CommandData::Label(Some("dup".to_string())));
        a1.borrow_mut().code = ':';

        let a2 = command_with_data(CommandData::Label(Some("dup".to_string())));
        a2.borrow_mut().code = ':';

        let head = link_commands(vec![a1, a2]);
        let mut context = ProcessingContext::default();

        let result = populate_label_map(head, &mut context);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate label `dup'"));
    }

    // populate_range_commands
    fn command_with_range(
        code: char,
        start: usize,
        end: usize,
        data: CommandData,
    ) -> Rc<RefCell<Command>> {
        Rc::new(RefCell::new(Command {
            code,
            addr1: Some(Address::Line(start)),
            addr2: Some(Address::Line(end)),
            data,
            ..Default::default()
        }))
    }

    #[test]
    fn test_range_address() {
        let cmd = command_with_range('p', 3, 5, CommandData::None);
        let mut context = ProcessingContext::default();
        assert_eq!(context.range_commands.len(), 0);

        populate_range_commands(Some(cmd.clone()), &mut context);

        assert_eq!(context.range_commands.len(), 1);

        // Verify it is the same command
        let rc = &context.range_commands[0];
        assert!(Rc::ptr_eq(rc, &cmd));

        // Verify addresses
        let cmd_ref = rc.borrow();

        assert!(matches!(cmd_ref.addr1, Some(Address::Line(3))));
        assert!(matches!(cmd_ref.addr2, Some(Address::Line(5))));
    }

    #[test]
    fn test_non_range_addresses_do_not_register() {
        let mut context = ProcessingContext::default();

        // Zero-address command
        let cmd0 = Rc::new(RefCell::new(Command {
            code: 'p',
            data: CommandData::None,
            ..Default::default()
        }));

        populate_range_commands(Some(cmd0), &mut context);
        assert!(context.range_commands.is_empty());

        // One-address command
        let cmd1 = Rc::new(RefCell::new(Command {
            code: 'p',
            addr1: Some(Address::Line(3)),
            data: CommandData::None,
            ..Default::default()
        }));

        populate_range_commands(Some(cmd1), &mut context);
        assert!(context.range_commands.is_empty());
    }

    #[test]
    fn test_range_address_outside_and_inside_block() {
        // Top-level range command: 1,2p
        let outer = command_with_range('p', 1, 2, CommandData::None);

        // Nested range command: 3,5p
        let nested = command_with_range('p', 3, 5, CommandData::None);

        // Block containing the nested range command
        let block = command_with_data(CommandData::BranchTarget(Some(nested.clone())));

        // Link outer -> block
        outer.borrow_mut().next = Some(block);

        let mut context = ProcessingContext::default();
        assert_eq!(context.range_commands.len(), 0);

        populate_range_commands(Some(outer.clone()), &mut context);

        // Two range commands must be found.
        assert_eq!(context.range_commands.len(), 2);

        // Verify both commands are present (order-independent).
        assert!(
            context
                .range_commands
                .iter()
                .any(|rc| Rc::ptr_eq(rc, &outer))
        );
        assert!(
            context
                .range_commands
                .iter()
                .any(|rc| Rc::ptr_eq(rc, &nested))
        );

        let nested_ref = nested.borrow();

        let addr1 = nested_ref.addr1.as_ref().expect("nested addr1 missing");
        assert!(matches!(addr1, Address::Line(3)));

        let addr2 = nested_ref.addr2.as_ref().expect("nested addr2 missing");
        assert!(matches!(addr2, Address::Line(5)));
    }

    // resolve_branch_targets
    #[test]
    fn test_branch_target_resolved() {
        let target = command_with_data(CommandData::Label(Some("end".to_string())));
        target.borrow_mut().code = ':';

        let branch = command_with_data(CommandData::Label(Some("end".to_string())));
        branch.borrow_mut().code = 'b';

        let head = link_commands(vec![branch.clone(), target.clone()]);
        let mut context = ProcessingContext::default();

        populate_label_map(head.clone(), &mut context).unwrap();
        let result = resolve_branch_targets(head, &mut context);
        assert!(result.is_ok());

        match &branch.borrow().data {
            CommandData::BranchTarget(Some(ptr)) => {
                assert!(Rc::ptr_eq(ptr, &target));
            }
            _ => panic!("Expected BranchTarget(Some(...))"),
        }
    }

    #[test]
    fn test_branch_target_missing_label_gives_error() {
        let branch = command_with_data(CommandData::Label(Some("nope".to_string())));
        branch.borrow_mut().code = 't';

        let mut context = ProcessingContext::default();
        let result = resolve_branch_targets(Some(branch), &mut context);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("can't find label for jump to `nope'"));
    }

    #[test]
    fn test_branch_with_no_label_resolves_to_none() {
        let branch = command_with_data(CommandData::Label(None));
        branch.borrow_mut().code = 'b';

        let mut context = ProcessingContext::default();
        let result = resolve_branch_targets(Some(branch.clone()), &mut context);

        assert!(result.is_ok());
        match &branch.borrow().data {
            CommandData::BranchTarget(None) => {} // ok
            _ => panic!("Expected BranchTarget(None)"),
        }
    }

    #[test]
    fn test_non_branch_label_is_unchanged() {
        let cmd = command_with_data(CommandData::Label(Some("unchanged".to_string())));
        cmd.borrow_mut().code = 'q'; // not a branch command

        let mut context = ProcessingContext::default();
        let result = resolve_branch_targets(Some(cmd.clone()), &mut context);
        assert!(result.is_ok());

        match &cmd.borrow().data {
            CommandData::Label(Some(label)) => assert_eq!(label, "unchanged"),
            _ => panic!("Expected Label(Some(...)) to remain unchanged"),
        }
    }

    #[test]
    fn test_branch_in_nested_block() {
        let label = command_with_data(CommandData::Label(Some("inner".to_string())));
        label.borrow_mut().code = ':';

        let branch = command_with_data(CommandData::Label(Some("inner".to_string())));
        branch.borrow_mut().code = 't';

        let block = command_with_data(CommandData::BranchTarget(Some(label.clone())));
        let head = link_commands(vec![branch.clone(), block]);

        let mut context = ProcessingContext::default();
        populate_label_map(Some(label.clone()), &mut context).unwrap();
        let result = resolve_branch_targets(head, &mut context);

        assert!(result.is_ok());
        match &branch.borrow().data {
            CommandData::BranchTarget(Some(ptr)) => assert!(Rc::ptr_eq(ptr, &label)),
            _ => panic!("Expected BranchTarget(Some(...))"),
        }
    }

    // compile_text_command
    #[test]
    fn test_compile_single_line_text_command() {
        let mut chars = make_char_provider("a\\");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"line1\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_posix_spaces_single_line() {
        let mut chars = make_char_provider("a \\ ");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext {
            posix: true,
            ..Default::default()
        };

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"line1\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_posix_incomplete() {
        let (mut lines, mut chars) = make_providers("i\\");
        let mut cmd = Command::default();
        let mut context = ProcessingContext {
            posix: true,
            ..Default::default()
        };
        let result = compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("incomplete command"));
    }

    #[test]
    fn test_compile_text_command_gnu_optional_backslash() {
        let mut chars = make_char_provider("athere");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"there\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_gnu_optional_backslash_spaces() {
        let mut chars = make_char_provider("a \t there");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"there\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_gnu_no_text() {
        let mut chars = make_char_provider("a");
        let mut lines = make_line_provider(&[]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        let result = compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains(ERR_TEXT_EXPECTED));
    }

    #[test]
    fn test_compile_text_command_gnu_optional_backslash_escape_eof() {
        let mut chars = make_char_provider("a\\");
        let mut lines = make_line_provider(&[]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_gnu_no_first_escape() {
        let mut chars = make_char_provider("a\\tom");
        let mut lines = make_line_provider(&[]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"tom\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_gnu_char_escapes() {
        let mut chars = make_char_provider("i\\>\\h\\elll\\bo\\nto\\");
        let mut lines = make_line_provider(&["all\\a", ""]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b">helll\x08o\nto\nall\x07\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_gnu_preserves_invalid_utf8_script_byte() {
        let mut chars = ScriptCharProvider::new(b"a\\\xC2");
        let mut lines = make_line_provider(&[]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"\xC2\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_two_line_text_command() {
        let mut chars = make_char_provider("a\\");
        let mut lines = make_line_provider(&["line1\\", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext::default();

        compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context).unwrap();
        match &cmd.data {
            CommandData::Text(text) => {
                assert_eq!(text.as_ref(), b"line1\nline2\n");
            }
            _ => panic!("Expected CommandData::Text"),
        }
    }

    #[test]
    fn test_compile_text_command_posix_without_backslash() {
        let mut chars = make_char_provider("a");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext {
            posix: true,
            ..Default::default()
        };

        let result = compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains(ERR_TEXT_EXPECTED));
    }

    #[test]
    fn test_compile_text_command_posix_with_trailing_chars() {
        let mut chars = make_char_provider("a \\ foo");
        let mut lines = make_line_provider(&["line1", "line2"]);
        let mut cmd = Command::default();
        let mut context = ProcessingContext {
            posix: true,
            ..Default::default()
        };

        let result = compile_text_command(&mut lines, &mut chars, &mut cmd, &mut context);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("extra characters after \\"));
    }

    // read_file_path
    #[test]
    fn test_read_existing_file_path() {
        let (lines, mut chars) = make_providers("r /etc/motd");

        let path = read_file_path(&lines, &mut chars).unwrap();
        assert_eq!(path.to_str().unwrap(), "/etc/motd");
    }

    #[test]
    fn test_read_missing_file_path() {
        let (lines, mut chars) = make_providers("w ");

        let err = read_file_path(&lines, &mut chars).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing filename in r/R/w/W commands")
        );
    }

    #[test]
    fn test_read_file_path_rejects_invalid_characters() {
        let lines = ScriptLineProvider::new(vec![]);
        let mut chars = ScriptCharProvider::new(b"w bad\xFFpath");

        let err = read_file_path(&lines, &mut chars).unwrap_err();
        assert!(err.to_string().contains("invalid characters file path"));
    }
}
