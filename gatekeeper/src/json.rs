//! Minimal, dependency-free JSON parser — just enough for skill-rules.json.
//! Supports objects, arrays, strings, numbers, booleans, and null.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Obj(m) => Some(m),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<Json, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Parser { chars, pos: 0 };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(format!("trailing data at position {}", p.pos));
    }
    Ok(v)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => Ok(Json::Str(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            other => Err(format!("unexpected token {other:?} at {}", self.pos)),
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.next(); // {
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.next();
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.next() != Some(':') {
                return Err(format!("expected ':' at {}", self.pos));
            }
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.next() {
                Some(',') => continue,
                Some('}') => break,
                other => return Err(format!("expected ',' or '}}', got {other:?}")),
            }
        }
        Ok(Json::Obj(map))
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.next(); // [
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.next();
            return Ok(Json::Arr(arr));
        }
        loop {
            let v = self.parse_value()?;
            arr.push(v);
            self.skip_ws();
            match self.next() {
                Some(',') => continue,
                Some(']') => break,
                other => return Err(format!("expected ',' or ']', got {other:?}")),
            }
        }
        Ok(Json::Arr(arr))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.next() != Some('"') {
            return Err(format!("expected string at {}", self.pos));
        }
        let mut s = String::new();
        while let Some(c) = self.next() {
            match c {
                '"' => return Ok(s),
                '\\' => match self.next() {
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('/') => s.push('/'),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some(other) => s.push(other),
                    None => return Err("unterminated escape".into()),
                },
                _ => s.push(c),
            }
        }
        Err("unterminated string".into())
    }

    fn parse_bool(&mut self) -> Result<Json, String> {
        if self.chars[self.pos..].starts_with(&['t', 'r', 'u', 'e']) {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.chars[self.pos..].starts_with(&['f', 'a', 'l', 's', 'e']) {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err(format!("invalid literal at {}", self.pos))
        }
    }

    fn parse_null(&mut self) -> Result<Json, String> {
        if self.chars[self.pos..].starts_with(&['n', 'u', 'l', 'l']) {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err(format!("invalid literal at {}", self.pos))
        }
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || "+-.eE".contains(c)) {
            self.pos += 1;
        }
        let slice: String = self.chars[start..self.pos].iter().collect();
        slice
            .parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("invalid number '{slice}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested() {
        let v = parse(r#"{"a": [1, "two", true, null], "b": {"c": 3.5}}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap().len(), 4);
        assert_eq!(v.get("b").unwrap().get("c"), Some(&Json::Num(3.5)));
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse("{} junk").is_err());
    }
}
