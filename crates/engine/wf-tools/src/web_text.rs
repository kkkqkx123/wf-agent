//! HTML document extraction shared by the web tools.
//!
//! Search backends extract short fragments from result markup while the fetch
//! handler renders whole pages. Both go through the same HTML5 DOM parser so a
//! snippet cannot drift from the page it points at.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{parse_document, Attribute, ExpandedName, QualName};
use regex::{Captures, Regex};

static ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(amp|lt|gt|quot|apos|nbsp|#\d+|#x[0-9a-fA-F]+);")
        .expect("invariant: fixed entity pattern must compile")
});

/// Decode named and numeric HTML entities while leaving unknown tokens intact.
pub fn decode_html_entities(input: &str) -> String {
    ENTITY_RE
        .replace_all(input, |captures: &Captures| {
            let token = &captures[1];
            match token {
                "amp" => "&".to_string(),
                "lt" => "<".to_string(),
                "gt" => ">".to_string(),
                "quot" => "\"".to_string(),
                "apos" | "#39" | "#x27" => "'".to_string(),
                "nbsp" => " ".to_string(),
                numeric if numeric.starts_with("#x") => {
                    let code = u32::from_str_radix(&numeric[2..], 16).ok();
                    code.and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| captures[0].to_string())
                }
                numeric => numeric[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| captures[0].to_string()),
            }
        })
        .into_owned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeId(usize);

#[derive(Clone, Debug)]
enum NodeData {
    Document,
    Element {
        name: QualName,
        attrs: Vec<Attribute>,
        template: Option<NodeId>,
        mathml_annotation: bool,
    },
    Text(String),
    Comment,
    Doctype,
}

#[derive(Clone, Debug)]
struct Node {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    data: NodeData,
}

#[derive(Clone, Debug)]
struct Dom {
    nodes: Vec<Node>,
    document: NodeId,
}

impl Default for Dom {
    fn default() -> Self {
        Self::new()
    }
}

impl Dom {
    fn new() -> Self {
        Self {
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }],
            document: NodeId(0),
        }
    }

    fn push(&mut self, data: NodeData) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            parent: None,
            children: Vec::new(),
            data,
        });
        id
    }

    fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.nodes[id.0].parent.take() else {
            return;
        };
        self.nodes[parent.0].children.retain(|child| *child != id);
    }

    fn link(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        self.nodes[child.0].parent = Some(parent);
        self.nodes[parent.0].children.push(child);
    }

    fn append_text(&mut self, parent: NodeId, text: &str) {
        if let Some(last) = self.nodes[parent.0].children.last().copied() {
            if let NodeData::Text(existing) = &mut self.nodes[last.0].data {
                existing.push_str(text);
                return;
            }
        }
        let id = self.push(NodeData::Text(text.to_string()));
        self.link(parent, id);
    }

    fn parse(input: &str) -> Self {
        parse_document(Self::new(), Default::default())
            .from_utf8()
            .one(input.as_bytes())
    }

    fn local_name(&self, id: NodeId) -> &str {
        match &self.nodes[id.0].data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        }
    }

    fn attrs(&self, id: NodeId) -> Option<&Vec<Attribute>> {
        match &self.nodes[id.0].data {
            NodeData::Element { attrs, .. } => Some(attrs),
            _ => None,
        }
    }
}

impl TreeSink for Dom {
    type Handle = NodeId;
    type Output = Self;

    fn finish(self) -> Self::Output {
        self
    }

    fn parse_error(&mut self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&mut self) -> Self::Handle {
        self.document
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> ExpandedName<'a> {
        match &self.nodes[target.0].data {
            NodeData::Element { name, .. } => name.expanded(),
            _ => panic!("tree builder asked for the name of a non-element"),
        }
    }

    fn create_element(
        &mut self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        self.push(NodeData::Element {
            name,
            attrs,
            template: None,
            mathml_annotation: flags.mathml_annotation_xml_integration_point,
        })
    }

    fn create_comment(&mut self, _text: StrTendril) -> Self::Handle {
        self.push(NodeData::Comment)
    }

    fn create_pi(&mut self, _target: StrTendril, _data: StrTendril) -> Self::Handle {
        self.push(NodeData::Comment)
    }

    fn append(&mut self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(id) => self.link(*parent, id),
            NodeOrText::AppendText(text) => self.append_text(*parent, &text),
        }
    }

    fn append_based_on_parent_node(
        &mut self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let parent = self.nodes[prev_element.0].parent.unwrap_or(*element);
        self.append(&parent, child);
    }

    fn append_doctype_to_document(
        &mut self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        let id = self.push(NodeData::Doctype);
        self.link(self.document, id);
    }

    fn get_template_contents(&mut self, target: &Self::Handle) -> Self::Handle {
        let existing = match &self.nodes[target.0].data {
            NodeData::Element { template, .. } => *template,
            _ => panic!("tree builder asked for template contents of a non-template"),
        };
        if let Some(id) = existing {
            return id;
        }
        let id = self.push(NodeData::Document);
        match &mut self.nodes[target.0].data {
            NodeData::Element { template, .. } => *template = Some(id),
            _ => panic!("tree builder asked for template contents of a non-template"),
        }
        id
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&mut self, _mode: QuirksMode) {}

    fn append_before_sibling(
        &mut self,
        sibling: &Self::Handle,
        new_node: NodeOrText<Self::Handle>,
    ) {
        let Some(parent) = self.nodes[sibling.0].parent else {
            return;
        };
        if let NodeOrText::AppendText(text) = &new_node {
            if let Some(index) = self.nodes[parent.0]
                .children
                .iter()
                .position(|child| child == sibling)
            {
                if index > 0 {
                    let previous = self.nodes[parent.0].children[index - 1];
                    if let NodeData::Text(existing) = &mut self.nodes[previous.0].data {
                        existing.push_str(&text);
                        return;
                    }
                }
            }
        }
        let id = match new_node {
            NodeOrText::AppendNode(id) => id,
            NodeOrText::AppendText(text) => self.push(NodeData::Text(text.to_string())),
        };
        self.detach(id);
        self.nodes[id.0].parent = Some(parent);
        let index = self.nodes[parent.0]
            .children
            .iter()
            .position(|child| child == sibling)
            .unwrap_or(self.nodes[parent.0].children.len());
        self.nodes[parent.0].children.insert(index, id);
    }

    fn add_attrs_if_missing(&mut self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let NodeData::Element {
            attrs: existing, ..
        } = &mut self.nodes[target.0].data
        else {
            return;
        };
        for attr in attrs {
            if existing.iter().any(|candidate| candidate.name == attr.name) {
                continue;
            }
            existing.push(attr);
        }
    }

    fn remove_from_parent(&mut self, target: &Self::Handle) {
        self.detach(*target);
    }

    fn reparent_children(&mut self, node: &Self::Handle, new_parent: &Self::Handle) {
        let children = std::mem::take(&mut self.nodes[node.0].children);
        for child in children {
            self.nodes[child.0].parent = Some(*new_parent);
            self.nodes[new_parent.0].children.push(child);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        match &self.nodes[handle.0].data {
            NodeData::Element {
                mathml_annotation, ..
            } => *mathml_annotation,
            _ => false,
        }
    }
}

fn attr_value<'a>(attrs: &'a [Attribute], local: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.name.local.as_ref() == local)
        .map(|attr| &*attr.value)
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for c in input.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

fn resolve_url(base: Option<&str>, href: &str) -> String {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with("data:")
        || href.starts_with("javascript:")
        || href.starts_with("#")
    {
        return String::new();
    }
    let Some(base) = base else {
        return href.to_string();
    };
    match reqwest::Url::parse(base) {
        Ok(url) => url
            .join(href)
            .map(|joined| joined.to_string())
            .unwrap_or_else(|_| href.to_string()),
        Err(_) => href.to_string(),
    }
}

/// An external link discovered in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlLink {
    pub text: String,
    pub url: String,
}

/// A simple element selector: a tag, an id, a class, or their combination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub class: Option<String>,
}

/// Parse comma-separated simple selectors. More complex CSS syntax is rejected
/// so an unsupported selector never silently degrades to a whole-page dump.
pub fn parse_selector(input: &str) -> Result<Vec<HtmlSelector>, String> {
    let mut selectors = Vec::new();
    for part in input.split(',') {
        let raw = part.trim();
        if raw.is_empty() || raw.chars().any(char::is_whitespace) {
            return Err(format!("invalid selector '{raw}'"));
        }
        let (tag, rest) = match raw.find(['#', '.']) {
            Some(index) => (&raw[..index], &raw[index..]),
            None => (raw, ""),
        };
        let mut id = None;
        let mut class = None;
        let mut tail = rest;
        if let Some(stripped) = tail.strip_prefix('#') {
            let end = stripped.find('.').unwrap_or(stripped.len());
            id = Some(stripped[..end].to_string());
            tail = &stripped[end..];
        }
        if let Some(stripped) = tail.strip_prefix('.') {
            class = Some(stripped.to_string());
            tail = "";
        }
        if !tail.is_empty()
            || (!tag.is_empty() && !valid_name(tag))
            || id.as_deref().is_some_and(|value| !valid_name(value))
            || class.as_deref().is_some_and(|value| !valid_name(value))
        {
            return Err(format!("invalid selector '{raw}'"));
        }
        selectors.push(HtmlSelector {
            tag: (!tag.is_empty()).then(|| tag.to_string()),
            id,
            class,
        });
    }
    Ok(selectors)
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Output shape of the page renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlRenderMode {
    Markdown,
    Plain,
}

/// Rendering settings for one fetch call.
#[derive(Debug, Clone, Default)]
pub struct HtmlRenderOptions {
    pub mode: HtmlRenderMode,
    pub selector: Option<Vec<HtmlSelector>>,
    pub max_chars: Option<usize>,
}

impl Default for HtmlRenderMode {
    fn default() -> Self {
        Self::Markdown
    }
}

/// Structured content extracted from one HTML document.
#[derive(Debug, Clone, Default)]
pub struct HtmlDocument {
    pub title: String,
    pub description: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub links: Vec<HtmlLink>,
    dom: Dom,
}

impl HtmlDocument {
    /// Parse HTML and collect head metadata and links. Parsing is tolerant of
    /// malformed markup by construction.
    pub fn parse(html: &str, base_url: Option<&str>) -> Self {
        let dom = Dom::parse(html);
        let mut document = Self {
            dom,
            ..Default::default()
        };
        document.collect_document_data(base_url);
        document
    }

    fn collect_document_data(&mut self, base_url: Option<&str>) {
        let mut stack = vec![self.dom.document];
        while let Some(id) = stack.pop() {
            let name = self.dom.local_name(id).to_string();
            match name.as_str() {
                "title" if self.title.is_empty() => {
                    let mut text = String::new();
                    self.collect_text(id, &mut text, false);
                    self.title = collapse_whitespace(&text);
                }
                "meta" => {
                    if let Some(attrs) = self.dom.attrs(id) {
                        let key = attr_value(attrs, "name")
                            .or_else(|| attr_value(attrs, "property"))
                            .unwrap_or("");
                        let value = attr_value(attrs, "content").unwrap_or("");
                        if !key.is_empty() && !value.is_empty() && self.metadata.len() < 32 {
                            self.metadata
                                .insert(key.to_string(), value.chars().take(500).collect());
                        }
                    }
                }
                "a" => {
                    if let Some(attrs) = self.dom.attrs(id) {
                        if let Some(href) = attr_value(attrs, "href") {
                            let url = resolve_url(base_url, href);
                            if !url.is_empty() && self.links.len() < 200 {
                                let mut text = String::new();
                                self.collect_text(id, &mut text, false);
                                let text = collapse_whitespace(&text);
                                self.links.push(HtmlLink {
                                    text: if text.is_empty() { url.clone() } else { text },
                                    url,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
            let children = self.dom.nodes[id.0].children.clone();
            stack.extend(children);
        }
        self.description = self.metadata.get("description").cloned();
    }

    fn collect_text(&self, id: NodeId, out: &mut String, block_breaks: bool) {
        match &self.dom.nodes[id.0].data {
            NodeData::Document => {}
            NodeData::Text(text) => out.push_str(text),
            NodeData::Comment | NodeData::Doctype => return,
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                if skipped(tag) {
                    return;
                }
                if block_breaks && is_block(tag) {
                    out.push(' ');
                }
            }
        }
        for child in &self.dom.nodes[id.0].children {
            self.collect_text(*child, out, block_breaks);
        }
    }

    /// Render the document body in the requested mode.
    pub fn render(&self, base_url: Option<&str>, options: &HtmlRenderOptions) -> String {
        let roots = self.content_roots(options.selector.as_deref());
        let mut out = String::new();
        for root in roots {
            self.render_node(root, base_url, options.mode, &mut out);
        }
        let rendered = match options.mode {
            HtmlRenderMode::Markdown => out.trim().to_string(),
            HtmlRenderMode::Plain => normalize_lines(&out),
        };
        match options.max_chars {
            Some(limit) if rendered.chars().count() > limit => {
                let prefix: String = rendered.chars().take(limit).collect();
                format!("{prefix}\n\n[content shortened to {limit} characters]")
            }
            _ => rendered,
        }
    }

    fn content_roots(&self, selector: Option<&[HtmlSelector]>) -> Vec<NodeId> {
        let Some(selectors) = selector else {
            return vec![self.dom.document];
        };
        let mut roots = Vec::new();
        let mut stack = vec![self.dom.document];
        while let Some(id) = stack.pop() {
            if selectors.iter().any(|selector| self.matches(id, selector)) {
                roots.push(id);
            }
            stack.extend(self.dom.nodes[id.0].children.iter().copied());
        }
        let candidates = roots.clone();
        roots.retain(|id| {
            !candidates
                .iter()
                .any(|other| other != id && self.is_descendant(*id, *other))
        });
        roots
    }

    fn is_descendant(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut current = self.dom.nodes[id.0].parent;
        while let Some(parent) = current {
            if parent == ancestor {
                return true;
            }
            current = self.dom.nodes[parent.0].parent;
        }
        false
    }

    fn matches(&self, id: NodeId, selector: &HtmlSelector) -> bool {
        if matches!(&self.dom.nodes[id.0].data, NodeData::Element { .. }) == false {
            return false;
        }
        if selector
            .tag
            .as_deref()
            .is_some_and(|tag| tag != self.dom.local_name(id))
        {
            return false;
        }
        let Some(attrs) = self.dom.attrs(id) else {
            return false;
        };
        if selector
            .id
            .as_deref()
            .is_some_and(|value| attr_value(attrs, "id") != Some(value))
        {
            return false;
        }
        selector.class.as_deref().is_none_or(|value| {
            attr_value(attrs, "class")
                .is_some_and(|classes| classes.split_whitespace().any(|class| class == value))
        })
    }

    fn render_node(&self, id: NodeId, base: Option<&str>, mode: HtmlRenderMode, out: &mut String) {
        match &self.dom.nodes[id.0].data {
            NodeData::Document => {
                for child in &self.dom.nodes[id.0].children {
                    self.render_node(*child, base, mode, out);
                }
            }
            NodeData::Text(text) => out.push_str(&collapse_whitespace(text)),
            NodeData::Comment | NodeData::Doctype => {}
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.as_ref();
                if skipped(tag) || hidden(attrs) {
                    return;
                }
                match tag {
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        let level: usize = tag[1..].parse().unwrap_or(1);
                        let text = self.inline_text(id, base, mode);
                        push_block(out, &format!("{} {}", "#".repeat(level), text));
                    }
                    "ul" | "ol" => self.render_list(id, tag == "ol", base, mode, out),
                    "li" => push_line(out, &format!("- {}", self.inline_text(id, base, mode))),
                    "blockquote" => {
                        let text = self.inline_text(id, base, mode);
                        for line in text.lines() {
                            push_line(out, &format!("> {line}"));
                        }
                    }
                    "pre" => {
                        let mut text = String::new();
                        self.collect_text(id, &mut text, false);
                        let text = text.trim();
                        if mode == HtmlRenderMode::Markdown {
                            push_block(out, &format!("```\n{text}\n```"));
                        } else {
                            push_block(out, text);
                        }
                    }
                    "table" => self.render_table(id, base, mode, out),
                    "hr" => push_block(out, "---"),
                    "br" => out.push('\n'),
                    "a" => {
                        let text = self.inline_text(id, base, mode);
                        let url = attr_value(attrs, "href")
                            .map(|href| resolve_url(base, href))
                            .unwrap_or_default();
                        if mode == HtmlRenderMode::Markdown && !url.is_empty() {
                            out.push_str(&format!("[{text}]({url})"));
                        } else {
                            out.push_str(&text);
                        }
                    }
                    "img" => {
                        let alt = attr_value(attrs, "alt").unwrap_or("");
                        let url = attr_value(attrs, "src")
                            .map(|src| resolve_url(base, src))
                            .unwrap_or_default();
                        if mode == HtmlRenderMode::Markdown && !url.is_empty() {
                            out.push_str(&format!("![{alt}]({url})"));
                        } else {
                            out.push_str(alt);
                        }
                    }
                    "strong" | "b" | "em" | "i" | "code" => {
                        let text = self.inline_text(id, base, mode);
                        if mode == HtmlRenderMode::Markdown {
                            let marker = match tag {
                                "strong" | "b" => "**",
                                "em" | "i" => "*",
                                _ => "`",
                            };
                            out.push_str(&format!("{marker}{text}{marker}"));
                        } else {
                            out.push_str(&text);
                        }
                    }
                    "p" | "div" | "section" | "article" | "main" | "figcaption" | "details" => {
                        let text = self.inline_text(id, base, mode);
                        push_block(out, &text);
                    }
                    _ => {
                        for child in &self.dom.nodes[id.0].children {
                            self.render_node(*child, base, mode, out);
                        }
                    }
                }
            }
        }
    }

    fn inline_text(&self, id: NodeId, base: Option<&str>, mode: HtmlRenderMode) -> String {
        let mut out = String::new();
        for child in &self.dom.nodes[id.0].children {
            self.render_node(*child, base, mode, &mut out);
        }
        collapse_whitespace(&out)
    }

    fn render_list(
        &self,
        id: NodeId,
        ordered: bool,
        base: Option<&str>,
        mode: HtmlRenderMode,
        out: &mut String,
    ) {
        let mut index = 1;
        for child in &self.dom.nodes[id.0].children {
            if self.dom.local_name(*child) != "li" {
                continue;
            }
            let text = self.inline_text(*child, base, mode);
            let prefix = if ordered {
                format!("{index}. ")
            } else {
                "- ".to_string()
            };
            push_line(out, &format!("{prefix}{text}"));
            index += 1;
        }
    }

    fn render_table(&self, id: NodeId, base: Option<&str>, mode: HtmlRenderMode, out: &mut String) {
        let mut rows = Vec::new();
        collect_cells(self, id, base, mode, "tr", &mut rows);
        if rows.is_empty() {
            return;
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        for (index, row) in rows.iter().enumerate() {
            let mut cells: Vec<String> = row.iter().cloned().collect();
            cells.resize(columns, String::new());
            let line = format!("| {} |", cells.join(" | "));
            push_line(out, &line);
            if index == 0 && mode == HtmlRenderMode::Markdown {
                push_line(out, &format!("| {} |", vec!["---"; columns].join(" | ")));
            }
        }
    }
}

fn collect_cells(
    document: &HtmlDocument,
    id: NodeId,
    base: Option<&str>,
    mode: HtmlRenderMode,
    target: &str,
    rows: &mut Vec<Vec<String>>,
) {
    if document.dom.local_name(id) == target {
        let mut cells = Vec::new();
        for child in &document.dom.nodes[id.0].children {
            let tag = document.dom.local_name(*child);
            if tag == "td" || tag == "th" {
                cells.push(document.inline_text(*child, base, mode));
            }
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        return;
    }
    for child in &document.dom.nodes[id.0].children {
        collect_cells(document, *child, base, mode, target, rows);
    }
}

fn skipped(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "noscript"
            | "svg"
            | "nav"
            | "footer"
            | "header"
            | "aside"
            | "form"
            | "button"
            | "template"
            | "iframe"
    )
}

fn hidden(attrs: &[Attribute]) -> bool {
    let hidden = attr_value(attrs, "hidden").is_some();
    let aria_hidden = attr_value(attrs, "aria-hidden") == Some("true");
    let display_none = attr_value(attrs, "style")
        .is_some_and(|style| style.to_ascii_lowercase().contains("display:none"));
    hidden || aria_hidden || display_none
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "main"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "br"
            | "tr"
            | "table"
            | "blockquote"
            | "pre"
    )
}

fn normalize_lines(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        let line = collapse_whitespace(line);
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

fn push_line(out: &mut String, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
}

fn push_block(out: &mut String, block: &str) {
    let block = block.trim();
    if block.is_empty() {
        return;
    }
    if !out.is_empty() {
        while out.ends_with('\n') {
            out.pop();
        }
        out.push_str("\n\n");
    }
    out.push_str(block);
}

/// Compact single-line text used for search result snippets.
pub fn html_to_text(input: &str) -> String {
    let document = HtmlDocument::parse(input, None);
    let mut text = String::new();
    document.collect_text(document.dom.document, &mut text, true);
    collapse_whitespace(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_html_entities("caf&#233; &#x27;x&#x27;"), "café 'x'");
    }

    #[test]
    fn keeps_unknown_entities_untouched() {
        assert_eq!(decode_html_entities("&custom; &amp;"), "&custom; &");
    }

    #[test]
    fn snippet_text_collapses_blocks() {
        let html = "<style>p{color:red}</style><h1>Title &amp; more</h1>\
                    <script>alert('x')</script><p>A&nbsp; &lt;b&gt;body&lt;/b&gt;</p>";
        assert_eq!(html_to_text(html), "Title & more A <b>body</b>");
    }

    #[test]
    fn renders_structured_markdown_with_absolute_links() {
        let html = "<html><head><title>T &amp; S</title>\
            <meta name='description' content='Summary'></head><body>\
            <nav>skip</nav><article><h2>Heading</h2><p>One <a href='/next'>link</a>.</p>\
            <ul><li>A</li><li>B</li></ul><pre><code>let x = 1;</code></pre>\
            <table><tr><th>K</th><th>V</th></tr><tr><td>a</td><td>1</td></tr></table>\
            </article><footer>bye</footer></body></html>";
        let document = HtmlDocument::parse(html, Some("https://example.com/path"));
        let markdown = document.render(
            Some("https://example.com/path"),
            &HtmlRenderOptions::default(),
        );
        assert_eq!(document.title, "T & S");
        assert_eq!(document.description.as_deref(), Some("Summary"));
        assert!(markdown.contains("## Heading"));
        assert!(markdown.contains("[link](https://example.com/next)"));
        assert!(markdown.contains("- A\n- B"));
        assert!(markdown.contains("```"));
        assert!(markdown.contains("| K | V |"));
        assert!(!markdown.contains("skip"));
        assert!(!markdown.contains("bye"));
        assert_eq!(document.links[0].url, "https://example.com/next");
    }

    #[test]
    fn selector_limits_rendered_content() {
        let html = "<main><p>keep</p></main><aside><p>drop</p></aside>";
        let document = HtmlDocument::parse(html, None);
        let selectors = parse_selector("main").expect("valid selector");
        let rendered = document.render(
            None,
            &HtmlRenderOptions {
                selector: Some(selectors),
                ..Default::default()
            },
        );
        assert_eq!(rendered, "keep");
    }

    #[test]
    fn rejects_complex_selectors() {
        assert!(parse_selector("div > p").is_err());
        assert!(parse_selector("main, #content").is_ok());
    }
}
