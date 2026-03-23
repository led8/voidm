use super::ast::*;
use super::lexer::{tokenize, Token};
use anyhow::{bail, Result};

pub fn parse(input: &str) -> Result<CypherAst> {
    let tokens = tokenize(input);
    let mut p = Parser { tokens, pos: 0 };
    p.parse_query()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<()> {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == kw.to_uppercase() => {
                self.advance();
                Ok(())
            }
            other => bail!("Expected '{}', got {}", kw, token_display(other)),
        }
    }

    fn try_keyword(&mut self, kw: &str) -> bool {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == kw.to_uppercase() => {
                self.advance();
                true
            }
            _ => false,
        }
    }

    fn try_ident(&mut self) -> Option<String> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                Some(s)
            }
            _ => None,
        }
    }

    fn parse_query(&mut self) -> Result<CypherAst> {
        let mut match_clauses = Vec::new();
        let mut where_clause = None;
        let mut return_exprs = Vec::new();
        let mut order_by = Vec::new();
        let mut limit = None;

        // Parse MATCH / OPTIONAL MATCH clauses
        loop {
            let optional = self.try_keyword("OPTIONAL");
            if self.try_keyword("MATCH") {
                let pattern = self.parse_pattern()?;
                match_clauses.push(MatchClause { pattern, optional });
            } else {
                if optional {
                    bail!("Expected MATCH after OPTIONAL");
                }
                break;
            }
        }

        // WITH (skip for now — minimal support)
        if self.try_keyword("WITH") {
            // Skip until MATCH or WHERE or RETURN
            while !matches!(self.peek(), Token::Keyword(k) if ["MATCH","WHERE","RETURN","ORDER","LIMIT"].contains(&k.as_str()))
            {
                if matches!(self.peek(), Token::EOF) {
                    break;
                }
                self.advance();
            }
        }

        // WHERE
        if self.try_keyword("WHERE") {
            where_clause = Some(self.parse_where()?);
        }

        // RETURN
        if self.try_keyword("RETURN") {
            return_exprs = self.parse_return_list()?;
        }

        // ORDER BY
        if self.try_keyword("ORDER") {
            self.expect_keyword("BY")?;
            order_by = self.parse_order_by()?;
        }

        // LIMIT
        if self.try_keyword("LIMIT") {
            match self.advance().clone() {
                Token::Integer(n) => limit = Some(n),
                other => bail!(
                    "Expected an integer after LIMIT, got {}",
                    token_display(&other)
                ),
            }
        }

        Ok(CypherAst {
            match_clauses,
            where_clause,
            return_exprs,
            order_by,
            limit,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // First node
        nodes.push(self.parse_node_spec()?);

        // Edge-node pairs
        loop {
            match self.peek() {
                Token::Dash | Token::Arrow => {
                    let edge = self.parse_edge_spec()?;
                    edges.push(edge);
                    nodes.push(self.parse_node_spec()?);
                }
                _ => break,
            }
        }

        Ok(Pattern { nodes, edges })
    }

    fn parse_node_spec(&mut self) -> Result<NodeSpec> {
        match self.peek() {
            Token::LParen => {
                self.advance();
            }
            other => bail!(
                "Expected '(' to start a node pattern, got {}",
                token_display(other)
            ),
        }

        let var = self.try_ident();
        let mut label = None;
        let mut props = Vec::new();

        if let Token::Colon = self.peek() {
            self.advance();
            label = self.try_ident();
        }

        if let Token::LBrace = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::RBrace | Token::EOF) {
                let key = self
                    .try_ident()
                    .ok_or_else(|| anyhow::anyhow!("Expected property key"))?;
                match self.advance() {
                    Token::Colon => {}
                    Token::Equals => {}
                    other => bail!(
                        "Expected ':' or '=' after property key, got {}",
                        token_display(other)
                    ),
                }
                let val = self.parse_prop_value()?;
                props.push((key, val));
                if let Token::Comma = self.peek() {
                    self.advance();
                }
            }
            if let Token::RBrace = self.peek() {
                self.advance();
            }
        }

        match self.peek() {
            Token::RParen => {
                self.advance();
            }
            other => bail!(
                "Expected ')' to close node pattern, got {}",
                token_display(other)
            ),
        }

        Ok(NodeSpec { var, label, props })
    }

    fn parse_edge_spec(&mut self) -> Result<EdgeSpec> {
        // Patterns: -[...]->  <-[...]-  -[...]-  ->  <-  -
        let starts_with_arrow = matches!(self.peek(), Token::Arrow);
        if starts_with_arrow {
            self.advance(); // consume Arrow (<-)
        }

        let _direction_start = !starts_with_arrow;
        let dash_before = matches!(self.peek(), Token::Dash);
        if dash_before {
            self.advance();
        }

        let mut var = None;
        let mut rel_type = None;
        let mut min_hops = None;
        let mut max_hops = None;

        // Optional [...]
        if let Token::LBracket = self.peek() {
            self.advance();
            var = self.try_ident();

            if let Token::Colon = self.peek() {
                self.advance();
                rel_type = self.try_ident().or_else(|| {
                    // might be uppercase keyword used as rel_type e.g. SUPPORTS
                    if let Token::Keyword(k) = self.peek().clone() {
                        self.advance();
                        Some(k)
                    } else {
                        None
                    }
                });
            }

            // Variable depth: *1..3 or *
            if let Token::Star = self.peek() {
                self.advance();
                if let Token::Integer(n) = self.peek().clone() {
                    self.advance();
                    min_hops = Some(n as u32);
                    if let Token::DotDot = self.peek() {
                        self.advance();
                        if let Token::Integer(m) = self.peek().clone() {
                            self.advance();
                            max_hops = Some(m as u32);
                        }
                    }
                }
            }

            match self.peek() {
                Token::RBracket => {
                    self.advance();
                }
                other => bail!(
                    "Expected ']' to close relationship pattern, got {}",
                    token_display(other)
                ),
            }
        }

        // Determine direction based on what follows
        let after_bracket_arrow = matches!(self.peek(), Token::Arrow);
        let after_bracket_dash = matches!(self.peek(), Token::Dash);

        let direction = if starts_with_arrow {
            // Started with <-, so Incoming
            if after_bracket_dash {
                self.advance();
            }
            EdgeDirection::Incoming
        } else if after_bracket_arrow {
            self.advance(); // consume ->
            EdgeDirection::Outgoing
        } else {
            if after_bracket_dash {
                self.advance();
            }
            EdgeDirection::Undirected
        };

        Ok(EdgeSpec {
            var,
            rel_type,
            direction,
            min_hops,
            max_hops,
        })
    }

    fn parse_prop_value(&mut self) -> Result<PropValue> {
        match self.advance().clone() {
            Token::StringLit(s) => Ok(PropValue::String(s)),
            Token::Integer(n) => Ok(PropValue::Integer(n)),
            Token::Keyword(k) if k == "TRUE" => Ok(PropValue::Bool(true)),
            Token::Keyword(k) if k == "FALSE" => Ok(PropValue::Bool(false)),
            Token::Keyword(k) if k == "NULL" => Ok(PropValue::Null),
            other => bail!(
                "Expected a property value (string, integer, true/false, null), got {}",
                token_display(&other)
            ),
        }
    }

    fn parse_where(&mut self) -> Result<WhereExpr> {
        let left = self.parse_where_atom()?;
        if self.try_keyword("AND") {
            let right = self.parse_where()?;
            Ok(WhereExpr::And(Box::new(left), Box::new(right)))
        } else if self.try_keyword("OR") {
            let right = self.parse_where()?;
            Ok(WhereExpr::Or(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_where_atom(&mut self) -> Result<WhereExpr> {
        if self.try_keyword("NOT") {
            let inner = self.parse_where_atom()?;
            return Ok(WhereExpr::Not(Box::new(inner)));
        }

        // var.prop OP value
        let var = self
            .try_ident()
            .ok_or_else(|| anyhow::anyhow!("Expected variable in WHERE clause"))?;
        self.advance(); // consume Dot
        let prop = self
            .try_ident()
            .ok_or_else(|| anyhow::anyhow!("Expected property name after '.'"))?;

        let op = self.parse_comp_op()?;
        let value = self.parse_prop_value()?;

        Ok(WhereExpr::Comparison(Comparison {
            var,
            prop,
            op,
            value,
        }))
    }

    fn parse_comp_op(&mut self) -> Result<CompOp> {
        match self.advance().clone() {
            Token::Equals => Ok(CompOp::Eq),
            Token::Keyword(k) if k == "CONTAINS" => Ok(CompOp::Contains),
            Token::Keyword(k) if k == "STARTS" => {
                self.expect_keyword("WITH")?;
                Ok(CompOp::StartsWith)
            }
            Token::Keyword(k) if k == "ENDS" => {
                self.expect_keyword("WITH")?;
                Ok(CompOp::EndsWith)
            }
            other => bail!(
                "Unknown comparison operator: {}. Supported: =, CONTAINS, STARTS WITH, ENDS WITH",
                token_display(&other)
            ),
        }
    }

    fn parse_return_list(&mut self) -> Result<Vec<ReturnExpr>> {
        let mut items = Vec::new();
        let _ = self.try_keyword("DISTINCT");

        loop {
            let item = self.parse_return_item()?;
            let alias = if self.try_keyword("AS") {
                self.try_ident()
            } else {
                None
            };
            items.push(ReturnExpr { expr: item, alias });

            if let Token::Comma = self.peek() {
                self.advance();
            } else {
                break;
            }
        }

        Ok(items)
    }

    fn parse_return_item(&mut self) -> Result<ReturnItem> {
        // COUNT(*) or COUNT(var)
        if let Token::Ident(name) = self.peek().clone() {
            if name.to_uppercase() == "COUNT" {
                self.advance();
                if let Token::LParen = self.peek() {
                    self.advance();
                    let inner = self.try_ident();
                    if let Token::RParen = self.peek() {
                        self.advance();
                    }
                    return Ok(ReturnItem::Count(inner));
                }
            }
        }

        let var = self
            .try_ident()
            .ok_or_else(|| anyhow::anyhow!("Expected variable in RETURN"))?;

        if let Token::Dot = self.peek() {
            self.advance();
            let prop = self
                .try_ident()
                .ok_or_else(|| anyhow::anyhow!("Expected property after '.'"))?;
            Ok(ReturnItem::Property(var, prop))
        } else {
            Ok(ReturnItem::Variable(var))
        }
    }

    fn parse_order_by(&mut self) -> Result<Vec<OrderBy>> {
        let mut items = Vec::new();
        loop {
            let item = self.parse_return_item()?;
            let desc = self.try_keyword("DESC");
            let _ = self.try_keyword("ASC");
            items.push(OrderBy { expr: item, desc });
            if let Token::Comma = self.peek() {
                self.advance();
            } else {
                break;
            }
        }
        Ok(items)
    }
}

fn token_display(t: &Token) -> String {
    match t {
        Token::Keyword(k) => format!("keyword '{}'", k),
        Token::Ident(s) => format!("identifier '{}'", s),
        Token::StringLit(s) => format!("string \"{}\"", s),
        Token::Integer(n) => format!("integer '{}'", n),
        Token::LParen => "'('".into(),
        Token::RParen => "')'".into(),
        Token::LBracket => "'['".into(),
        Token::RBracket => "']'".into(),
        Token::LBrace => "'{'".into(),
        Token::RBrace => "'}'".into(),
        Token::Arrow => "'->' or '<-'".into(),
        Token::Dash => "'-'".into(),
        Token::Dot => "'.'".into(),
        Token::Comma => "','".into(),
        Token::Equals => "'='".into(),
        Token::Colon => "':'".into(),
        Token::Star => "'*'".into(),
        Token::DotDot => "'..'".into(),
        Token::Label(l) => format!("label ':{}'", l),
        Token::RelType(r) => format!("rel-type '[:{}'", r),
        Token::Newline => "newline".into(),
        Token::EOF => "end of query".into(),
    }
}
