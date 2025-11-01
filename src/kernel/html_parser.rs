/// Simple HTML parser for rOSt web browser
/// Supports basic tags: html, body, h1-h6, p, a, br, div, b, i, ul, ol, li

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Element(ElementData),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub node_type: NodeType,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new_element(tag: &str, attrs: BTreeMap<String, String>, children: Vec<Node>) -> Node {
        Node {
            node_type: NodeType::Element(ElementData {
                tag_name: tag.to_string(),
                attributes: attrs,
            }),
            children,
        }
    }

    pub fn new_text(text: &str) -> Node {
        Node {
            node_type: NodeType::Text(text.to_string()),
            children: Vec::new(),
        }
    }
}

/// Simple HTML tokenizer
pub struct Parser {
    pos: usize,
    input: String,
}

impl Parser {
    pub fn new(input: String) -> Parser {
        Parser { pos: 0, input }
    }

    /// Parse HTML into a DOM tree
    pub fn parse(&mut self) -> Node {
        let mut nodes = self.parse_nodes();

        // If we got exactly one node, return it
        if nodes.len() == 1 {
            nodes.remove(0)
        } else {
            // Otherwise, wrap in a root element
            Node::new_element("html", BTreeMap::new(), nodes)
        }
    }

    /// Parse a sequence of sibling nodes
    fn parse_nodes(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        loop {
            self.skip_whitespace();
            if self.eof() || self.starts_with("</") {
                break;
            }
            nodes.push(self.parse_node());
        }
        nodes
    }

    /// Parse a single node (element or text)
    fn parse_node(&mut self) -> Node {
        if self.current_char() == '<' {
            // Skip DOCTYPE and comments
            if self.starts_with("<!") {
                self.skip_doctype_or_comment();
                // After skipping, try parsing the next node
                if !self.eof() {
                    return self.parse_node();
                } else {
                    return Node::new_text("");
                }
            }
            self.parse_element()
        } else {
            self.parse_text()
        }
    }

    /// Skip DOCTYPE declarations and HTML comments
    fn skip_doctype_or_comment(&mut self) {
        if self.starts_with("<!") {
            // Skip until we find '>'
            while !self.eof() && self.current_char() != '>' {
                self.consume_char();
            }
            if !self.eof() {
                self.consume_char(); // consume the '>'
            }
        }
    }

    /// Parse an element with opening tag, children, and closing tag
    fn parse_element(&mut self) -> Node {
        // Opening tag
        assert_eq!(self.consume_char(), '<');
        let tag_name = self.parse_tag_name();
        let attrs = self.parse_attributes();
        assert_eq!(self.consume_char(), '>');

        // Self-closing tags
        if tag_name == "br" || tag_name == "img" || tag_name == "hr" {
            return Node::new_element(&tag_name, attrs, Vec::new());
        }

        // Children
        let children = self.parse_nodes();

        // Closing tag
        self.skip_whitespace();
        if self.starts_with("</") {
            assert_eq!(self.consume_char(), '<');
            assert_eq!(self.consume_char(), '/');
            let close_tag = self.parse_tag_name();
            assert_eq!(close_tag, tag_name, "Mismatched closing tag");
            assert_eq!(self.consume_char(), '>');
        }

        Node::new_element(&tag_name, attrs, children)
    }

    /// Parse text content
    fn parse_text(&mut self) -> Node {
        let mut text = String::new();
        while !self.eof() && self.current_char() != '<' {
            text.push(self.consume_char());
        }
        // Trim excessive whitespace but preserve single spaces
        let trimmed = text.trim().to_string();
        Node::new_text(&trimmed)
    }

    /// Parse tag name (alphanumeric + hyphen)
    fn parse_tag_name(&mut self) -> String {
        let mut tag = String::new();
        while !self.eof() {
            let c = self.current_char();
            if c.is_alphanumeric() || c == '-' {
                tag.push(self.consume_char().to_ascii_lowercase());
            } else {
                break;
            }
        }
        tag
    }

    /// Parse element attributes
    fn parse_attributes(&mut self) -> BTreeMap<String, String> {
        let mut attrs = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.current_char() == '>' {
                break;
            }
            let (name, value) = self.parse_attr();
            attrs.insert(name, value);
        }
        attrs
    }

    /// Parse a single attribute: name="value"
    fn parse_attr(&mut self) -> (String, String) {
        let name = self.parse_tag_name();
        self.skip_whitespace();

        let value = if self.current_char() == '=' {
            self.consume_char();
            self.skip_whitespace();
            self.parse_attr_value()
        } else {
            String::new()
        };

        (name, value)
    }

    /// Parse attribute value (quoted string)
    fn parse_attr_value(&mut self) -> String {
        let quote = self.consume_char();
        assert!(quote == '"' || quote == '\'');

        let mut value = String::new();
        while !self.eof() && self.current_char() != quote {
            value.push(self.consume_char());
        }

        assert_eq!(self.consume_char(), quote);
        value
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while !self.eof() && self.current_char().is_whitespace() {
            self.consume_char();
        }
    }

    /// Get current character without consuming
    fn current_char(&self) -> char {
        self.input[self.pos..].chars().next().unwrap_or('\0')
    }

    /// Consume and return current character
    fn consume_char(&mut self) -> char {
        let c = self.current_char();
        self.pos += c.len_utf8();
        c
    }

    /// Check if input starts with given string
    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    /// Check if we've reached end of input
    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_html() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>".to_string();
        let mut parser = Parser::new(html);
        let dom = parser.parse();

        if let NodeType::Element(ref e) = dom.node_type {
            assert_eq!(e.tag_name, "html");
        } else {
            panic!("Expected element");
        }
    }
}
