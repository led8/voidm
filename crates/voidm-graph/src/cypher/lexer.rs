/// Strip `/* ... */` block comments and `//` line comments from a Cypher query.
pub fn strip_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            // Block comment: skip until */
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            // Line comment: skip until newline
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(String), // MATCH, WHERE, RETURN, etc.
    Ident(String),   // variable names, property names
    Label(String),   // :Memory (after colon)
    RelType(String), // [:SUPPORTS] content
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Arrow, // -> or <-
    Dash,  // -
    Dot,   // .
    Comma,
    Equals,
    Colon,
    Star,
    DotDot, // ..
    Integer(i64),
    StringLit(String),
    Newline,
    EOF,
}

const KEYWORDS: &[&str] = &[
    "MATCH", "WHERE", "RETURN", "ORDER", "BY", "LIMIT", "WITH", "AND", "OR", "NOT", "AS",
    "DISTINCT", "ASC", "DESC", "OPTIONAL", "CREATE", "MERGE", "SET", "DELETE", "REMOVE", "DROP",
    "IN", "STARTS", "ENDS", "CONTAINS", "IS", "NULL", "TRUE", "FALSE",
];

/// Tokenize a comment-stripped Cypher query.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // String literals
        if c == '\'' || c == '"' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    s.push(chars[i]);
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            i += 1; // closing quote
            tokens.push(Token::StringLit(s));
            continue;
        }

        // Numbers
        if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            if c == '-' {
                i += 1;
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            tokens.push(Token::Integer(s.parse().unwrap_or(0)));
            continue;
        }

        // Identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            if KEYWORDS.contains(&upper.as_str()) {
                tokens.push(Token::Keyword(upper));
            } else {
                tokens.push(Token::Ident(word));
            }
            continue;
        }

        // .. (before single dot)
        if c == '.' && i + 1 < chars.len() && chars[i + 1] == '.' {
            tokens.push(Token::DotDot);
            i += 2;
            continue;
        }

        // -> arrow
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '>' {
            tokens.push(Token::Arrow);
            i += 2;
            continue;
        }

        // <- arrow (we push Arrow for left-pointing too, direction tracked in parser)
        if c == '<' && i + 1 < chars.len() && chars[i + 1] == '-' {
            tokens.push(Token::Arrow);
            i += 2;
            continue;
        }

        match c {
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            '{' => {
                tokens.push(Token::LBrace);
                i += 1;
            }
            '}' => {
                tokens.push(Token::RBrace);
                i += 1;
            }
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Equals);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Dash);
                i += 1;
            }
            '>' => {
                i += 1;
            } // trailing > after ], skip
            '<' => {
                i += 1;
            } // leading < before -, handled above
            _ => {
                i += 1;
            } // skip unknown
        }
    }

    tokens.push(Token::EOF);
    tokens
}
