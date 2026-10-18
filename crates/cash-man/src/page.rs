//! A page of the manual: its front matter, any text before its first section, and its
//! sections.
//!
//! A page is markdown. Its front matter, between two `---` lines, holds `key: value`
//! lines; then come its `##` sections, each running to the next. A command's page has
//! sections from [`Heading`] alone, in that order, each at most once, so a reader always
//! knows where to look; a topic's sections are its own.

/// The widest line of code a page may hold, its four spaces of markdown counted.
///
/// Code is drawn as written, never wrapped, eight columns in (`render::CODE_INDENT`),
/// and a page must read in an 80-column terminal.
pub const CODE_WIDTH: usize = 76;

/// What a page is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// A command cash has: a builtin or a bundled tool.
    Command,
    /// Something about cash that no one command owns.
    Topic,
}

/// The sections of a command's page, in the order they come. "See also" is not among
/// them: it is made from the page's `see:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Heading {
    /// The forms the command is used in, a line each with what that form does.
    Syntax,
    /// The common uses, each a sentence and the command.
    Examples,
    /// What a reader needs to understand to use it well.
    HowItWorks,
    /// Every option and operand, and the exit statuses.
    Options,
    /// The rest, for the reader who needs it.
    Advanced,
    /// Where cash differs from the real command, and what it adds.
    DifferentInCash,
}

impl Heading {
    /// Every heading, in the order a page has them.
    pub const ALL: [Self; 6] = [
        Self::Syntax,
        Self::Examples,
        Self::HowItWorks,
        Self::Options,
        Self::Advanced,
        Self::DifferentInCash,
    ];

    /// The heading as a page writes it, after `## `.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Syntax => "Syntax",
            Self::Examples => "Examples",
            Self::HowItWorks => "How it works",
            Self::Options => "Options",
            Self::Advanced => "Advanced",
            Self::DifferentInCash => "Different in cash",
        }
    }

    /// The heading `title` names, in any case.
    #[must_use]
    pub fn named(title: &str) -> Option<Self> {
        let title = title.trim();
        Self::ALL
            .into_iter()
            .find(|heading| heading.title().eq_ignore_ascii_case(title))
    }
}

/// A `##` section of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The title, as written after `## `.
    pub title: String,
    /// The markdown under it, to the next `##` section, its `###` subsections and all.
    pub body: String,
}

impl Section {
    /// Which of a command page's sections this is.
    #[must_use]
    pub fn heading(&self) -> Option<Heading> {
        Heading::named(&self.title)
    }
}

/// A page of the manual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The file it came from, without `.md`: the name `man` takes for a topic.
    pub file: String,
    /// What it is about.
    pub subject: Subject,
    /// The commands it covers: its `name:` or `names:`, else its file's name.
    pub names: Vec<String>,
    /// A topic's title.
    pub title: String,
    /// One line saying what it is.
    pub summary: String,
    /// A command's kind, as a reader would put it: "Bash builtin", "Text tool".
    pub kind: String,
    /// The pages to read next.
    pub see: Vec<String>,
    /// Whether it is a draft, made from the command's `--help` and not yet written.
    pub draft: bool,
    /// The markdown before the first section.
    pub intro: String,
    /// The sections, in the page's order.
    pub sections: Vec<Section>,
    /// Front matter lines it does not know.
    unknown: Vec<String>,
}

impl Page {
    /// The page in `text`, from the file `file` (without `.md`). A page always reads:
    /// what is wrong with one is for [`Page::problems`] to say.
    #[must_use]
    pub fn parse(file: &str, subject: Subject, text: &str) -> Self {
        let text = text.replace("\r\n", "\n");
        let (front, body) = split_front_matter(&text);
        let mut page = Self {
            file: file.to_owned(),
            subject,
            names: Vec::new(),
            title: String::new(),
            summary: String::new(),
            kind: String::new(),
            see: Vec::new(),
            draft: false,
            intro: String::new(),
            sections: Vec::new(),
            unknown: Vec::new(),
        };
        for line in front.lines().filter(|line| !line.trim().is_empty()) {
            let Some((key, value)) = line.split_once(':') else {
                page.unknown.push(line.to_owned());
                continue;
            };
            let value = value.trim();
            let words = || -> Vec<String> { value.split_whitespace().map(str::to_owned).collect() };
            match key.trim() {
                "name" | "names" => page.names = words(),
                "title" => value.clone_into(&mut page.title),
                "summary" => value.clone_into(&mut page.summary),
                "kind" => value.clone_into(&mut page.kind),
                "see" => page.see = words(),
                "draft" => page.draft = matches!(value, "true" | "yes"),
                _ => page.unknown.push(line.to_owned()),
            }
        }
        if page.names.is_empty() {
            page.names.push(file.to_owned());
        }
        let (intro, sections) = split_sections(body);
        page.intro = intro;
        page.sections = sections;
        page
    }

    /// The section under `heading`, on a command's page.
    #[must_use]
    pub fn section(&self, heading: Heading) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| section.heading() == Some(heading))
    }

    /// What is wrong with the page's shape: its front matter, and on a command's page,
    /// text before the first section and sections that are not among [`Heading`],
    /// repeated or out of order. An empty section and a line of code wider than
    /// [`CODE_WIDTH`] are wrong on any page.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut problems: Vec<String> = self
            .unknown
            .iter()
            .map(|line| format!("front matter: not known: {line}"))
            .collect();
        if self.summary.is_empty() {
            problems.push("front matter: no summary".to_owned());
        }
        match self.subject {
            Subject::Command => {
                if self.kind.is_empty() {
                    problems.push("front matter: no kind".to_owned());
                }
                if !self.intro.trim().is_empty() {
                    problems.push("text before the first section".to_owned());
                }
                problems.extend(self.order_problems());
            }
            Subject::Topic => {
                if self.title.is_empty() {
                    problems.push("front matter: no title".to_owned());
                }
            }
        }
        for section in &self.sections {
            if section.body.trim().is_empty() {
                problems.push(format!("{}: empty", section.title));
            }
            for line in section.body.lines().filter(|line| line.starts_with("    ")) {
                let width = line.chars().count();
                if width > CODE_WIDTH {
                    problems.push(format!(
                        "{}: a line of code {width} columns wide: {}",
                        section.title,
                        line.trim()
                    ));
                }
            }
        }
        problems
    }

    /// A command page's sections against [`Heading`]: each known, once, in order.
    fn order_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut last: Option<Heading> = None;
        for section in &self.sections {
            let Some(heading) = section.heading() else {
                problems.push(format!(
                    "not a section of a command's page: {}",
                    section.title
                ));
                continue;
            };
            match last {
                Some(before) if before == heading => {
                    problems.push(format!("{} twice", heading.title()));
                }
                Some(before) if before > heading => {
                    problems.push(format!("{} after {}", heading.title(), before.title()));
                }
                _ => last = Some(heading),
            }
        }
        problems
    }
}

/// The front matter between a first line `---` and the next, and the rest; no front
/// matter when the text does not start with `---`.
fn split_front_matter(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return ("", text);
    };
    if let Some(body) = rest.strip_prefix("---\n") {
        return ("", body);
    }
    if let Some(split) = rest.split_once("\n---\n") {
        return split;
    }
    rest.strip_suffix("\n---")
        .map_or(("", text), |front| (front, ""))
}

/// The text before the first `##` heading, and the sections. A `##` inside a fenced
/// code block is code.
fn split_sections(body: &str) -> (String, Vec<Section>) {
    let mut intro = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut fenced = false;
    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let title = (!fenced)
            .then(|| line.strip_prefix("## "))
            .flatten()
            .map(str::trim);
        match (title, sections.last_mut()) {
            (Some(title), _) => sections.push(Section {
                title: title.to_owned(),
                body: String::new(),
            }),
            (None, Some(section)) => section.body.push_str(line),
            (None, None) => intro.push_str(line),
        }
    }
    for section in &mut sections {
        section.body = section.body.trim_matches('\n').to_owned();
    }
    (intro.trim_matches('\n').to_owned(), sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(text: &str) -> Page {
        Page::parse("cd", Subject::Command, text)
    }

    const FRONT: &str =
        "---\nname: cd\nsummary: Change directory.\nkind: Bash builtin\nsee: pwd z\n---\n";

    #[test]
    fn a_page_is_its_front_matter_and_its_sections() {
        let page = command(&format!(
            "{FRONT}\n## Syntax\n\n    cd DIR\n\n## Examples\n\nGo home:\n\n    cd\n\n### More\n\nText.\n"
        ));
        assert_eq!(page.names, ["cd"]);
        assert_eq!(page.summary, "Change directory.");
        assert_eq!(page.kind, "Bash builtin");
        assert_eq!(page.see, ["pwd", "z"]);
        assert!(!page.draft);
        assert_eq!(page.sections.len(), 2);
        assert_eq!(page.sections[0].body, "    cd DIR");
        let examples = page.section(Heading::Examples).unwrap();
        assert_eq!(examples.body, "Go home:\n\n    cd\n\n### More\n\nText.");
        assert!(page.problems().is_empty(), "{:?}", page.problems());
    }

    #[test]
    fn names_default_to_the_file_and_a_draft_says_so() {
        let page = command("---\nsummary: s\nkind: k\ndraft: yes\n---\n## Syntax\n\n    cd\n");
        assert_eq!(page.names, ["cd"]);
        assert!(page.draft);
        let page = command("---\nnames: 7z 7za\nsummary: s\nkind: k\n---\n");
        assert_eq!(page.names, ["7z", "7za"]);
    }

    #[test]
    fn a_command_page_keeps_its_sections_in_order() {
        let page = command(&format!(
            "{FRONT}Intro.\n## Examples\n\nx\n## Syntax\n\nx\n## Syntax\n\nx\n## Usage\n\nx\n## Options\n"
        ));
        assert_eq!(
            page.problems(),
            [
                "text before the first section",
                "Syntax after Examples",
                "Syntax after Examples",
                "not a section of a command's page: Usage",
                "Options: empty",
            ]
        );
        let twice = command(&format!("{FRONT}## Syntax\n\nx\n## Syntax\n\ny\n"));
        assert_eq!(twice.problems(), ["Syntax twice"]);
    }

    #[test]
    fn front_matter_must_be_whole_and_known() {
        let page = command("---\nname: cd\ncolour: blue\nno colon here\n---\n");
        assert_eq!(
            page.problems(),
            [
                "front matter: not known: colour: blue",
                "front matter: not known: no colon here",
                "front matter: no summary",
                "front matter: no kind",
            ]
        );
        let topic = Page::parse(
            "paths",
            Subject::Topic,
            "---\nsummary: s\n---\nIntro.\n## Any\n\nx\n",
        );
        assert_eq!(topic.problems(), ["front matter: no title"]);
        assert_eq!(topic.intro, "Intro.");
    }

    #[test]
    fn code_must_fit_eighty_columns_where_it_is_drawn() {
        let wide = format!("    {}", "x".repeat(CODE_WIDTH - 3));
        let page = command(&format!(
            "{FRONT}## Syntax\n\n{wide}\n    {}\n",
            "y".repeat(CODE_WIDTH - 4)
        ));
        assert_eq!(page.problems().len(), 1, "{:?}", page.problems());
        assert!(page.problems()[0].starts_with("Syntax: a line of code 77 columns wide"));
    }

    #[test]
    fn a_heading_in_fenced_code_is_code_and_crlf_reads_as_lf() {
        let page = command(&format!(
            "{}## Syntax\r\n\r\n```\r\n## not a heading\r\n```\r\n",
            FRONT.replace('\n', "\r\n")
        ));
        assert_eq!(page.sections.len(), 1);
        assert_eq!(page.sections[0].body, "```\n## not a heading\n```");
        assert_eq!(page.summary, "Change directory.");
    }

    #[test]
    fn a_page_without_front_matter_is_all_body() {
        let page = command("## Syntax\n\n    cd\n");
        assert_eq!(page.summary, "");
        assert_eq!(page.sections.len(), 1);
    }
}
