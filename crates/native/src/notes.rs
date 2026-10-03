//! Markdown to the HTML Apple Notes stores as a note's body.
//!
//! Ported from krmj22/macos-mcp `markdownToHtml` (MIT): escape first, then
//! headings, bullet and numbered lists, and inline bold/strike/code, one line
//! at a time. Changed from the port: lines are `<div>`s as Notes writes them,
//! not `<br>`-joined; the title is `<div><h1>` because that is what renders
//! title-sized (eliotshea/notes-mcp spike findings §12); `[ ]`/`[x]` are
//! dropped, since a scripted body write turns checklists into bullets anyway;
//! and `_` never italicises, so `snake_case` survives.

/// A whole note body: the title, then `markdown` below it.
pub(crate) fn note_body(title: &str, markdown: &str) -> String {
    let mut body = format!("<div><h1>{}</h1></div>", escape(title.trim()));
    body.push_str(&to_html(markdown));
    body
}

/// Body HTML for `markdown`, to set as a note's body or append to one.
pub(crate) fn to_html(markdown: &str) -> String {
    let mut html = String::new();
    // The list being built, if any: its tag and items.
    let mut list: Option<(&str, Vec<String>)> = None;

    for raw in markdown.lines() {
        let line = raw.trim();
        let item = bullet(line)
            .map(|text| ("ul", text))
            .or_else(|| numbered(line).map(|text| ("ol", text)));
        match (item, &mut list) {
            (Some((tag, text)), Some((open, items))) if *open == tag => items.push(inline(text)),
            (Some((tag, text)), _) => {
                flush(&mut html, list.take());
                list = Some((tag, vec![inline(text)]));
            }
            (None, _) => {
                flush(&mut html, list.take());
                html.push_str(&block(line));
            }
        }
    }
    flush(&mut html, list);
    html
}

fn flush(html: &mut String, list: Option<(&str, Vec<String>)>) {
    if let Some((tag, items)) = list {
        html.push_str(&format!("<{tag}>"));
        for item in items {
            html.push_str(&format!("<li>{item}</li>"));
        }
        html.push_str(&format!("</{tag}>"));
    }
}

/// A non-list line: a heading, a blank, or a plain line.
fn block(line: &str) -> String {
    if line.is_empty() {
        return "<div><br></div>".to_string();
    }
    let level = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
        // Notes renders three heading sizes; deeper ones clamp to the smallest.
        let h = level.min(3);
        return format!("<div><h{h}>{}</h{h}></div>", inline(line[level..].trim()));
    }
    format!("<div>{}</div>", inline(line))
}

fn bullet(line: &str) -> Option<&str> {
    let rest = ["- ", "* ", "+ ", "• "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))?;
    let rest = ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|box_| rest.strip_prefix(box_))
        .unwrap_or(rest);
    Some(rest.trim())
}

fn numbered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    rest.strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))
        .map(str::trim)
}

fn inline(text: &str) -> String {
    let text = escape(text);
    let text = pair(&text, "**", "<b>", "</b>");
    let text = pair(&text, "__", "<b>", "</b>");
    let text = pair(&text, "~~", "<s>", "</s>");
    let text = pair(&text, "`", "<tt>", "</tt>");
    pair(&text, "*", "<i>", "</i>")
}

/// Replaces `marker`s in open/close pairs; an unpaired last one stays literal,
/// so "5 * 3" is not half an italic.
fn pair(text: &str, marker: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(marker).collect();
    let pairs = (parts.len() - 1) / 2;
    let mut out = String::from(parts[0]);
    for (i, part) in parts.iter().enumerate().skip(1) {
        if i > pairs * 2 {
            out.push_str(marker);
        } else {
            out.push_str(if i % 2 == 1 { open } else { close });
        }
        out.push_str(part);
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cake list that started this: a heading, then bullets as one list,
    /// not a line of run-together text.
    #[test]
    fn a_shopping_list_becomes_one_bulleted_list_under_the_title() {
        let body = note_body("Cake", "Ingredients\n- flour\n- [ ] eggs\n* sugar");
        assert_eq!(
            body,
            "<div><h1>Cake</h1></div><div>Ingredients</div>\
             <ul><li>flour</li><li>eggs</li><li>sugar</li></ul>"
        );
    }

    #[test]
    fn numbered_and_bulleted_lists_stay_separate() {
        assert_eq!(
            to_html("1. preheat\n2) mix\n- extra"),
            "<ol><li>preheat</li><li>mix</li></ol><ul><li>extra</li></ul>"
        );
    }

    /// Notes parses the body as HTML: a pasted `<script>` or a bare `&` must
    /// show as text.
    #[test]
    fn markup_in_the_text_is_escaped() {
        assert_eq!(
            to_html("<script>x</script> & co"),
            "<div>&lt;script&gt;x&lt;/script&gt; &amp; co</div>"
        );
    }

    #[test]
    fn inline_styles_pair_up_and_strays_stay_literal() {
        assert_eq!(
            to_html("**bold** ~~gone~~ `code` *it* 5 * 3"),
            "<div><b>bold</b> <s>gone</s> <tt>code</tt> <i>it</i> 5 * 3</div>"
        );
        assert_eq!(to_html("my_var_name"), "<div>my_var_name</div>");
    }

    #[test]
    fn headings_clamp_and_blank_lines_keep_their_space() {
        assert_eq!(
            to_html("## Steps\n\n#### deep"),
            "<div><h2>Steps</h2></div><div><br></div><div><h3>deep</h3></div>"
        );
    }
}
