//! Hand-written parser for simplified GQL (Phase 12.4).

use crate::ast::{
    EdgePattern, NodePattern, Pattern, Predicate, PropertyMatcher, Query, ReturnClause, WhereClause,
};
use rbuilder_error::{Error, Result};
use rbuilder_graph::schema::{EdgeType, NodeType};

/// Parse a simplified GQL query string.
pub fn parse(input: &str) -> Result<Query> {
    let mut parser = Parser::new(input);
    parser.parse_query()
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse_query(&mut self) -> Result<Query> {
        self.skip_whitespace();
        self.expect_keyword("MATCH")?;

        let mut patterns = Vec::new();
        loop {
            patterns.push(self.parse_pattern()?);
            self.skip_whitespace();
            if self.starts_with_keyword("WHERE") || self.starts_with_keyword("RETURN") {
                break;
            }
            if self.peek_char().is_none() {
                break;
            }
        }

        let where_clause = if self.starts_with_keyword("WHERE") {
            self.pos += 5;
            Some(self.parse_where()?)
        } else {
            None
        };

        self.skip_whitespace();
        self.expect_keyword("RETURN")?;
        let return_clause = self.parse_return()?;

        self.skip_whitespace();
        let limit = if self.starts_with_keyword("LIMIT") {
            self.pos += 5;
            self.skip_whitespace();
            Some(self.parse_usize()?)
        } else {
            None
        };

        self.skip_whitespace();
        if self.peek_char().is_some() {
            return Err(Error::InvalidQuery(format!(
                "unexpected trailing input at position {}",
                self.pos
            )));
        }

        Ok(Query {
            patterns,
            where_clause,
            return_clause,
            limit,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        let node = self.parse_node_pattern()?;
        let mut hops = Vec::new();
        loop {
            self.skip_whitespace();
            if !self.consume('-') {
                break;
            }
            let edge = self.parse_edge_pattern()?;
            self.expect_str("->")?;
            let target = self.parse_node_pattern()?;
            hops.push((edge, target));
        }
        Ok(Pattern { node, hops })
    }

    fn parse_node_pattern(&mut self) -> Result<NodePattern> {
        self.expect_char('(')?;
        self.skip_whitespace();
        let variable = self.parse_ident()?;
        self.skip_whitespace();

        let node_type = if self.consume(':') {
            Some(self.parse_node_type()?)
        } else {
            None
        };
        self.skip_whitespace();

        let properties = if self.consume('{') {
            let props = self.parse_inline_properties()?;
            self.expect_char('}')?;
            props
        } else {
            std::collections::HashMap::new()
        };

        self.skip_whitespace();
        self.expect_char(')')?;
        Ok(NodePattern {
            variable,
            node_type,
            properties,
        })
    }

    fn parse_inline_properties(
        &mut self,
    ) -> Result<std::collections::HashMap<String, PropertyMatcher>> {
        let mut map = std::collections::HashMap::new();
        loop {
            self.skip_whitespace();
            if self.consume('}') {
                self.pos -= 1;
                break;
            }
            let key = self.parse_ident()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let value = self.parse_string_or_ident()?;
            map.insert(key, PropertyMatcher::Equals(value));
            self.skip_whitespace();
            if !self.consume(',') {
                break;
            }
        }
        Ok(map)
    }

    fn parse_edge_pattern(&mut self) -> Result<EdgePattern> {
        self.expect_char('[')?;
        self.expect_char(':')?;
        let edge_type = self.parse_edge_type()?;
        let (min_hops, max_hops) = if self.consume('*') {
            self.skip_whitespace();
            if matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                let min = self.parse_usize()?;
                self.expect_str("..")?;
                let max = self.parse_usize()?;
                (min, Some(max))
            } else {
                (1, None)
            }
        } else {
            (1, Some(1))
        };
        self.expect_char(']')?;
        Ok(EdgePattern {
            edge_type,
            min_hops,
            max_hops,
        })
    }

    fn parse_where(&mut self) -> Result<WhereClause> {
        let mut predicates = Vec::new();
        loop {
            self.skip_whitespace();
            predicates.push(self.parse_predicate()?);
            self.skip_whitespace();
            if !self.starts_with_keyword("AND") {
                break;
            }
            self.pos += 3;
        }
        Ok(WhereClause { predicates })
    }

    fn parse_predicate(&mut self) -> Result<Predicate> {
        let variable = self.parse_ident()?;
        self.expect_char('.')?;
        let property = self.parse_ident()?;
        self.skip_whitespace();
        if self.starts_with_keyword("LIKE") {
            self.pos += 4;
            self.skip_whitespace();
            let pattern = self.parse_quoted_string()?;
            Ok(Predicate::Like {
                variable,
                property,
                pattern,
            })
        } else {
            self.expect_char('=')?;
            self.skip_whitespace();
            let value = self.parse_quoted_string()?;
            Ok(Predicate::Equals {
                variable,
                property,
                value,
            })
        }
    }

    fn parse_return(&mut self) -> Result<ReturnClause> {
        self.skip_whitespace();
        let mut variables = Vec::new();
        variables.push(self.parse_ident()?);
        loop {
            self.skip_whitespace();
            if !self.consume(',') {
                break;
            }
            self.skip_whitespace();
            variables.push(self.parse_ident()?);
        }
        Ok(ReturnClause { variables })
    }

    fn parse_node_type(&mut self) -> Result<NodeType> {
        let ident = self.parse_ident()?;
        parse_node_type_name(&ident)
    }

    fn parse_edge_type(&mut self) -> Result<EdgeType> {
        let ident = self.parse_ident()?;
        parse_edge_type_name(&ident)
    }

    fn parse_ident(&mut self) -> Result<String> {
        self.skip_whitespace();
        let start = self.pos;
        let first = self
            .peek_char()
            .ok_or_else(|| Error::InvalidQuery("expected identifier".into()))?;
        if !first.is_ascii_alphabetic() && first != '_' {
            return Err(Error::InvalidQuery(format!(
                "expected identifier at position {}",
                self.pos
            )));
        }
        while matches!(self.peek_char(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_usize(&mut self) -> Result<usize> {
        self.skip_whitespace();
        let start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(Error::InvalidQuery("expected number".into()));
        }
        self.input[start..self.pos]
            .parse::<usize>()
            .map_err(|e: std::num::ParseIntError| Error::InvalidQuery(e.to_string()))
    }

    fn parse_quoted_string(&mut self) -> Result<String> {
        self.skip_whitespace();
        let quote = self
            .peek_char()
            .ok_or_else(|| Error::InvalidQuery("expected quoted string".into()))?;
        if quote != '\'' && quote != '"' {
            return Err(Error::InvalidQuery(format!(
                "expected quoted string at position {}",
                self.pos
            )));
        }
        self.pos += 1;
        let mut value = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == quote {
                self.pos += 1;
                return Ok(value);
            }
            if ch == '\\' {
                self.pos += 1;
                let escaped = self
                    .peek_char()
                    .ok_or_else(|| Error::InvalidQuery("bad escape".into()))?;
                value.push(escaped);
                self.pos += 1;
            } else {
                value.push(ch);
                self.pos += 1;
            }
        }
        Err(Error::InvalidQuery("unterminated string".into()))
    }

    fn parse_string_or_ident(&mut self) -> Result<String> {
        self.skip_whitespace();
        match self.peek_char() {
            Some('\'') | Some('"') => self.parse_quoted_string(),
            _ => self.parse_ident(),
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<()> {
        self.skip_whitespace();
        if self.starts_with_keyword(kw) {
            self.pos += kw.len();
            Ok(())
        } else {
            Err(Error::InvalidQuery(format!("expected keyword {kw}")))
        }
    }

    fn starts_with_keyword(&self, kw: &str) -> bool {
        let rest = self.input[self.pos..].trim_start();
        rest.len() >= kw.len()
            && rest.as_bytes()[..kw.len()].eq_ignore_ascii_case(kw.as_bytes())
            && (rest.len() == kw.len()
                || !rest.as_bytes()[kw.len()].is_ascii_alphanumeric()
                    && rest.as_bytes()[kw.len()] != b'_')
    }

    fn expect_str(&mut self, s: &str) -> Result<()> {
        self.skip_whitespace();
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            Ok(())
        } else {
            Err(Error::InvalidQuery(format!("expected '{s}'")))
        }
    }

    fn expect_char(&mut self, ch: char) -> Result<()> {
        self.skip_whitespace();
        if self.peek_char() == Some(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(Error::InvalidQuery(format!("expected '{ch}'")))
        }
    }

    fn consume(&mut self, ch: char) -> bool {
        self.skip_whitespace();
        if self.peek_char() == Some(ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
}

fn parse_node_type_name(name: &str) -> Result<NodeType> {
    match name.to_ascii_lowercase().as_str() {
        "function" => Ok(NodeType::Function),
        "class" => Ok(NodeType::Class),
        "struct" => Ok(NodeType::Struct),
        "enum" => Ok(NodeType::Enum),
        "interface" => Ok(NodeType::Interface),
        "module" => Ok(NodeType::Module),
        "variable" => Ok(NodeType::Variable),
        "file" => Ok(NodeType::File),
        "configkey" | "config" => Ok(NodeType::ConfigKey),
        "typealias" => Ok(NodeType::TypeAlias),
        "macro" => Ok(NodeType::Macro),
        "import" => Ok(NodeType::Import),
        "table" => Ok(NodeType::Table),
        "dependency" => Ok(NodeType::Dependency),
        "job" => Ok(NodeType::Job),
        "buildstep" => Ok(NodeType::BuildStep),
        "ansibleplaybook" | "playbook" => Ok(NodeType::AnsiblePlaybook),
        "ansibleplay" => Ok(NodeType::AnsiblePlay),
        "ansibletask" | "task" => Ok(NodeType::AnsibleTask),
        "ansiblerole" => Ok(NodeType::AnsibleRole),
        "ansiblehandler" => Ok(NodeType::AnsibleHandler),
        "ansiblevariable" => Ok(NodeType::AnsibleVariable),
        "ansibletemplate" => Ok(NodeType::AnsibleTemplate),
        "chefcookbook" | "cookbook" => Ok(NodeType::ChefCookbook),
        "chefrecipe" | "recipe" => Ok(NodeType::ChefRecipe),
        "chefresource" | "resource" => Ok(NodeType::ChefResource),
        "chefattribute" | "attribute" => Ok(NodeType::ChefAttribute),
        "cheftemplate" => Ok(NodeType::ChefTemplate),
        "chefcustomresource" => Ok(NodeType::ChefCustomResource),
        "puppetmodule" | "puppetmodules" => Ok(NodeType::PuppetModule),
        "puppetclass" | "puppetclasses" => Ok(NodeType::PuppetClass),
        "puppetdefinedtype" => Ok(NodeType::PuppetDefinedType),
        "puppetresource" => Ok(NodeType::PuppetResource),
        "puppetvariable" => Ok(NodeType::PuppetVariable),
        "puppetfact" => Ok(NodeType::PuppetFact),
        _ => Err(Error::InvalidQuery(format!("unknown node type: {name}"))),
    }
}

fn parse_edge_type_name(name: &str) -> Result<EdgeType> {
    match name.to_ascii_uppercase().as_str() {
        "CALLS" => Ok(EdgeType::Calls),
        "CONTAINS" => Ok(EdgeType::Contains),
        "USES" => Ok(EdgeType::Uses),
        "IMPLEMENTS" => Ok(EdgeType::Implements),
        "EXTENDS" => Ok(EdgeType::Extends),
        "REFERENCES" => Ok(EdgeType::References),
        "INSTANTIATES" => Ok(EdgeType::Instantiates),
        "MODIFIES" => Ok(EdgeType::Modifies),
        "USESCONFIG" => Ok(EdgeType::UsesConfig),
        "DEFINEDIN" => Ok(EdgeType::DefinedIn),
        "DEPENDSON" => Ok(EdgeType::DependsOn),
        _ => Err(Error::InvalidQuery(format!("unknown edge type: {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_match() {
        let q = parse("MATCH (f:Function) WHERE f.name = 'main' RETURN f").unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.patterns[0].node.variable, "f");
        assert_eq!(q.patterns[0].node.node_type, Some(NodeType::Function));
        assert_eq!(q.return_clause.variables, vec!["f"]);
    }

    #[test]
    fn test_parse_multi_hop() {
        let q = parse("MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b").unwrap();
        assert_eq!(q.patterns[0].hops.len(), 1);
        let (edge, node) = &q.patterns[0].hops[0];
        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.min_hops, 1);
        assert_eq!(edge.max_hops, Some(2));
        assert_eq!(node.variable, "b");
    }

    #[test]
    fn test_parse_limit() {
        let q = parse("MATCH (n:Function) WHERE n.name = 'foo' RETURN n LIMIT 10").unwrap();
        assert_eq!(q.limit, Some(10));
    }
}
