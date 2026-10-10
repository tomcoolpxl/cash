//! Each tool's messages, in English, from its own `en-US.ftl` compiled into cash.
//!
//! In uutils a tool's messages come from a bundle uucore builds out of `.ftl` files its
//! build script finds beside it in the cargo registry, under `uu_<tool>-<version>`. A
//! tool that is cash's own code is not there on a clean machine, so a release printed
//! the ids instead (`sort: sort-cannot-read`, `Usage: tee-usage`). Here each tool keeps
//! its file and its `translate!` calls: the macro looks the id up in the calling tool's
//! `MESSAGES` first, and in uucore's own messages after, as uucore's bundle did.

use std::cell::RefCell;
use std::collections::HashMap;

use fluent::{FluentArgs, FluentBundle, FluentResource};

/// One tool's messages: the text of its `en-US.ftl`.
pub(crate) struct Messages(&'static str);

thread_local! {
    /// The bundle of each tool's messages, made when the tool first asks for one;
    /// `None` when its file does not parse at all.
    static BUNDLES: RefCell<HashMap<usize, Option<FluentBundle<FluentResource>>>> =
        RefCell::new(HashMap::new());
}

impl Messages {
    /// The messages in `ftl`.
    pub(crate) const fn new(ftl: &'static str) -> Self {
        Self(ftl)
    }

    /// The message `id` with `args` put in; uucore's when this tool has no such message,
    /// and the id itself when neither has.
    pub(crate) fn get(&self, id: &str, args: Option<FluentArgs<'_>>) -> String {
        let found = BUNDLES.with(|bundles| {
            let mut bundles = bundles.borrow_mut();
            let bundle = bundles
                .entry(self.0.as_ptr() as usize)
                .or_insert_with(|| bundle_of(self.0))
                .as_ref()?;
            let message = bundle.get_message(id)?.value()?;
            let mut errors = Vec::new();
            Some(
                bundle
                    .format_pattern(message, args.as_ref(), &mut errors)
                    .into_owned(),
            )
        });
        found.unwrap_or_else(|| match args {
            Some(args) => uucore::locale::get_message_with_args(id, args),
            None => uucore::locale::get_message(id),
        })
    }
}

/// A bundle of the messages in `ftl`, without the Unicode isolation marks Fluent puts
/// around what is filled in, as uucore makes its own.
fn bundle_of(ftl: &'static str) -> Option<FluentBundle<FluentResource>> {
    // A file with a mistake still gives the messages around it.
    let resource = FluentResource::try_new(ftl.to_owned()).unwrap_or_else(|(resource, _)| resource);
    let english: unic_langid::LanguageIdentifier = "en-US".parse().ok()?;
    let mut bundle = FluentBundle::new(vec![english]);
    bundle.set_use_isolating(false);
    bundle.add_resource(resource).ok()?;
    Some(bundle)
}

/// A value for a message, as uucore's `translate!` passes it: a number Fluent can hold
/// exactly as a number, so the message can pick its plural form; any other text as it is.
pub(crate) fn value(text: String) -> fluent::FluentValue<'static> {
    if let Some(number) = uucore::locale::exact_fluent_integer(&text) {
        number.into()
    } else if uucore::locale::is_integer_literal(&text) {
        // An integer Fluent's f64 cannot hold exactly keeps its digits as text.
        text.into()
    } else if let Ok(number) = text.parse::<f64>() {
        number.into()
    } else {
        text.into()
    }
}

/// The message `id` of the tool whose `MESSAGES` is in scope where it is used, with any
/// `"name" => value` pairs filled in; a value that is a number goes in as one.
macro_rules! translate {
    ($id:expr) => {
        MESSAGES.get($id, None)
    };
    ($id:expr, $($key:expr => $value:expr),+ $(,)?) => {{
        let mut args = fluent::FluentArgs::new();
        $(
            args.set($key, $crate::messages::value($value.to_string()));
        )+
        MESSAGES.get($id, Some(args))
    }};
}

/// As [`translate!`], every value going in as text, never reread as a number.
macro_rules! translate_text {
    ($id:expr, $($key:expr => $value:expr),+ $(,)?) => {{
        let mut args = fluent::FluentArgs::new();
        $(
            args.set($key, $value.to_string());
        )+
        MESSAGES.get($id, Some(args))
    }};
}

pub(crate) use {translate, translate_text};
