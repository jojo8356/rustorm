//! Parser, validator and Rust type generator for the supported `.rsm` subset.
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}
impl std::error::Error for Diagnostic {}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Ident(String),
    String(String),
    Number(String),
    Symbol(char),
    Newline,
    Eof,
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    line: usize,
    col: usize,
}
fn err(t: &Token, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        line: t.line,
        column: t.col,
        message: message.into(),
    }
}

fn lex(input: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut out = vec![];
    let mut it = input.chars().peekable();
    let (mut line, mut col) = (1, 1);
    while let Some(ch) = it.next() {
        let (l, c) = (line, col);
        col += 1;
        let kind = match ch {
            ' ' | '\t' | '\r' => continue,
            '\n' => {
                line += 1;
                col = 1;
                Kind::Newline
            }
            '/' if it.peek() == Some(&'/') => {
                it.next();
                col += 1;
                while let Some(&x) = it.peek() {
                    if x == '\n' {
                        break;
                    }
                    it.next();
                    col += 1;
                }
                continue;
            }
            '/' if it.peek() == Some(&'*') => {
                it.next();
                col += 1;
                let mut closed = false;
                while let Some(x) = it.next() {
                    col += 1;
                    if x == '\n' {
                        line += 1;
                        col = 1;
                        out.push(Token {
                            kind: Kind::Newline,
                            line: line - 1,
                            col: 1,
                        });
                    }
                    if x == '*' && it.peek() == Some(&'/') {
                        it.next();
                        col += 1;
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(Diagnostic {
                        line: l,
                        column: c,
                        message: "unterminated comment".into(),
                    });
                }
                continue;
            }
            '"' => {
                let mut value = String::new();
                let mut closed = false;
                while let Some(x) = it.next() {
                    col += 1;
                    match x {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => {
                            let escaped = it.next().ok_or_else(|| Diagnostic {
                                line: l,
                                column: c,
                                message: "unterminated string".into(),
                            })?;
                            col += 1;
                            value.push(match escaped {
                                '"' => '"',
                                '\\' => '\\',
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                _ => {
                                    return Err(Diagnostic {
                                        line,
                                        column: col - 1,
                                        message: "unsupported string escape".into(),
                                    })
                                }
                            });
                        }
                        '\n' | '\r' => {
                            return Err(Diagnostic {
                                line: l,
                                column: c,
                                message: "newline in string".into(),
                            })
                        }
                        x if x.is_control() => {
                            return Err(Diagnostic {
                                line,
                                column: col - 1,
                                message: "control character in string".into(),
                            })
                        }
                        _ => value.push(x),
                    }
                }
                if !closed {
                    return Err(Diagnostic {
                        line: l,
                        column: c,
                        message: "unterminated string".into(),
                    });
                }
                Kind::String(value)
            }
            x if x.is_ascii_alphabetic() || x == '_' => {
                let mut s = String::from(x);
                while it
                    .peek()
                    .is_some_and(|x| x.is_ascii_alphanumeric() || *x == '_')
                {
                    s.push(it.next().unwrap());
                    col += 1;
                }
                Kind::Ident(s)
            }
            x if x.is_ascii_digit() || x == '-' => {
                let mut s = String::from(x);
                while it.peek().is_some_and(|x| x.is_ascii_digit() || *x == '.') {
                    s.push(it.next().unwrap());
                    col += 1;
                }
                Kind::Number(s)
            }
            x if "{}()[]@?=,.:".contains(x) => Kind::Symbol(x),
            _ => {
                return Err(Diagnostic {
                    line: l,
                    column: c,
                    message: format!("unexpected character {ch:?}"),
                })
            }
        };
        out.push(Token {
            kind,
            line: l,
            col: c,
        });
    }
    out.push(Token {
        kind: Kind::Eof,
        line,
        col,
    });
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub name: String,
    pub table: String,
    pub fields: Vec<Field>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeType {
    pub name: String,
    pub fields: Vec<Field>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub models: Vec<Model>,
    pub views: Vec<View>,
    pub composites: Vec<CompositeType>,
    pub enums: Vec<Enum>,
    pub generator_output: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: String,
    pub table: String,
    pub fields: Vec<Field>,
    /// Virtual relation fields; these are not physical database columns.
    pub relations: Vec<Relation>,
    /// Single or composite indexes.
    pub indexes: Vec<Index>,
    /// Composite unique constraints via @@unique.
    pub unique_constraints: Vec<UniqueConstraint>,
    /// Logical field names of an @@id; empty when a scalar field has @id.
    pub primary_key: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub field: String,
    pub fields: Vec<String>,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferentialAction {
    Cascade,
    Restrict,
    NoAction,
    SetNull,
}
impl ReferentialAction {
    pub fn sql(self) -> &'static str {
        match self {
            Self::Cascade => "CASCADE",
            Self::Restrict => "RESTRICT",
            Self::NoAction => "NO ACTION",
            Self::SetNull => "SET NULL",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub field: String,
    pub references: String,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub name: String,
    pub target: String,
    /// Explicit relation name, shared by both ends when present.
    pub relation_name: Option<String>,
    pub list: bool,
    pub nullable: bool,
    pub foreign_key: Option<ForeignKey>,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub column: String,
    pub ty: Scalar,
    pub nullable: bool,
    pub id: bool,
    pub unique: bool,
    pub default: Option<DefaultValue>,
    pub native_type: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub mapped: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueConstraint {
    pub fields: Vec<String>,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Int,
    BigInt,
    Float,
    Boolean,
    String,
    DateTime,
    Bytes,
    Json,
    Decimal { precision: Option<u32>, scale: Option<u32> },
    Enum(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultValue {
    Autoincrement,
    Now,
    String(String),
    Number(String),
    Boolean(bool),
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}
impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn next(&mut self) -> Token {
        let t = self.tokens[self.at].clone();
        if t.kind != Kind::Eof {
            self.at += 1;
        }
        t
    }
    fn symbol(&mut self, c: char) -> Result<(), Diagnostic> {
        let t = self.next();
        if t.kind == Kind::Symbol(c) {
            Ok(())
        } else {
            Err(err(&t, format!("expected '{c}'")))
        }
    }
    fn ident(&mut self) -> Result<(String, Token), Diagnostic> {
        let t = self.next();
        match &t.kind {
            Kind::Ident(v) => Ok((v.clone(), t)),
            _ => Err(err(&t, "expected identifier")),
        }
    }
    fn string(&mut self) -> Result<String, Diagnostic> {
        let t = self.next();
        match &t.kind {
            Kind::String(v) => Ok(v.clone()),
            _ => Err(err(&t, "expected string")),
        }
    }
    fn is(&self, c: char) -> bool {
        self.peek().kind == Kind::Symbol(c)
    }
    fn is_single_at(&self) -> bool {
        self.peek().kind == Kind::Symbol('@') && self.tokens.get(self.at+1).map(|t| t.kind != Kind::Symbol('@')).unwrap_or(true)
    }
    fn lines(&mut self) {
        while self.peek().kind == Kind::Newline {
            self.next();
        }
    }
    fn end_line(&mut self) -> Result<(), Diagnostic> {
        self.lines();
        Ok(())
    }
    fn config(&mut self, block: &str) -> Result<Option<String>, Diagnostic> {
        self.ident()?;
        self.symbol('{')?;
        self.lines();
        let mut seen = HashSet::new();
        let mut provider = None;
        let mut output = None;
        while !self.is('}') {
            if self.peek().kind == Kind::Eof {
                return Err(err(self.peek(), "unterminated configuration block"));
            }
            let (key, token) = self.ident()?;
            if !seen.insert(key.clone()) {
                return Err(err(&token, "duplicate configuration key"));
            }
            self.symbol('=')?;
            match (block, key.as_str()) {
                ("datasource", "provider") | ("generator", "provider") => {
                    provider = Some(self.string()?);
                }
                ("generator", "output") => {
                    let value = self.string()?;
                    if value.is_empty() || value.contains('\0') {
                        return Err(err(&token, "invalid generator output"));
                    }
                    output = Some(value);
                }
                ("datasource", "url") => {
                    let (func, tok) = self.ident()?;
                    if func != "env" {
                        return Err(err(&tok, "url must use env(\"VARIABLE\")"));
                    }
                    self.symbol('(')?;
                    let v = self.string()?;
                    if v.is_empty() {
                        return Err(err(&tok, "environment variable name cannot be empty"));
                    }
                    self.symbol(')')?;
                }
                _ => return Err(err(&token, format!("unsupported {block} property '{key}'"))),
            }
            self.end_line()?;
        }
        self.symbol('}')?;
        let expected = if block == "datasource" {
            "postgresql"
        } else {
            "rustorm-client"
        };
        if provider.as_deref() != Some(expected) {
            return Err(err(
                self.peek(),
                format!("{block} provider must be '{expected}'"),
            ));
        }
        Ok(output)
    }
    fn relation_arguments(&mut self) -> Result<(Option<String>, Option<ForeignKey>), Diagnostic> {
        self.symbol('(')?;
        let relation_name = if matches!(&self.peek().kind, Kind::String(_)) {
            let tok = self.peek().clone();
            let name = self.string()?;
            if name.is_empty() || name.contains('\0') {
                return Err(err(
                    &tok,
                    "relation name must be nonempty and contain no NUL",
                ));
            }
            Some(name)
        } else {
            None
        };
        if self.is(')') {
            self.next();
            if relation_name.is_none() {
                return Err(err(self.peek(), "empty relation attribute"));
            }
            return Ok((relation_name, None));
        }
        if relation_name.is_some() {
            self.symbol(',')?;
        }
        let mut fields = None;
        let mut references = None;
        let mut on_delete = None;
        let mut on_update = None;
        let mut seen = HashSet::new();
        loop {
            let (name, token) = self.ident()?;
            if !seen.insert(name.clone()) {
                return Err(err(&token, "duplicate relation argument"));
            }
            self.symbol(':')?;
            match name.as_str() {
                "fields" | "references" => {
                    self.symbol('[')?;
                    let mut values = Vec::new();
                    if !self.is(']') {
                        loop {
                            let (value, _) = self.ident()?;
                            values.push(value);
                            if self.is(',') { self.next(); } else { break; }
                        }
                    }
                    self.symbol(']')?;
                    let joined = values.join(",");
                    if name == "fields" {
                        fields = Some(joined);
                    } else {
                        references = Some(joined);
                    }
                }
                "onDelete" | "onUpdate" => {
                    let (value, tok) = self.ident()?;
                    let action = match value.as_str() {
                        "Cascade" => ReferentialAction::Cascade,
                        "Restrict" => ReferentialAction::Restrict,
                        "NoAction" => ReferentialAction::NoAction,
                        "SetNull" => ReferentialAction::SetNull,
                        "SetDefault" => return Err(err(&tok, "SET DEFAULT is not supported; relations must be immediate and without SET DEFAULT")),
                        _ => return Err(err(&tok, "unsupported referential action")),
                    };
                    if name == "onDelete" {
                        on_delete = Some(action);
                    } else {
                        on_update = Some(action);
                    }
                }
                _ => return Err(err(&token, "unsupported relation argument")),
            }
            if !self.is(',') {
                break;
            }
            self.next();
        }
        self.symbol(')')?;
        Ok((
            relation_name,
            Some(ForeignKey {
                field: fields.ok_or_else(|| err(self.peek(), "relation requires fields"))?,
                references: references
                    .ok_or_else(|| err(self.peek(), "relation requires references"))?,
                on_delete: on_delete
                    .ok_or_else(|| err(self.peek(), "relation requires explicit onDelete"))?,
                on_update: on_update
                    .ok_or_else(|| err(self.peek(), "relation requires explicit onUpdate"))?,
            }),
        ))
    }
    fn model(&mut self) -> Result<Model, Diagnostic> {
        let (name, name_token) = self.ident()?;
        self.symbol('{')?;
        self.lines();
        let mut model = Model {
            table: name.clone(),
            name,
            fields: vec![],
            relations: vec![],
            indexes: vec![],
            unique_constraints: vec![],
            primary_key: vec![],
        };
        let mut has_map = false;
        let mut has_compound_id = false;
        while !self.is('}') {
            if self.peek().kind == Kind::Eof {
                return Err(err(self.peek(), "unterminated model"));
            }
            if self.is('@') {
                self.symbol('@')?;
                self.symbol('@')?;
                let (attr, tok) = self.ident()?;
                match attr.as_str() {
                    "map" => {
                        if has_map {
                            return Err(err(&tok, "duplicate @@map"));
                        }
                        has_map = true;
                        self.symbol('(')?;
                        model.table = self.string()?;
                        self.symbol(')')?;
                    }
                    "id" => {
                        if has_compound_id {
                            return Err(err(&tok, "duplicate @@id"));
                        }
                        has_compound_id = true;
                        self.symbol('(')?;
                        self.symbol('[')?;
                        loop {
                            let (field, _) = self.ident()?;
                            model.primary_key.push(field);
                            if !self.is(',') {
                                break;
                            }
                            self.next();
                        }
                        self.symbol(']')?;
                        self.symbol(')')?;
                    }
                    "index" => {
                        self.symbol('(')?;
                        self.symbol('[')?;
                        let mut fields = Vec::new();
                        let first_tok;
                        let (field, tok) = self.ident()?;
                        fields.push(field);
                        first_tok = tok;
                        while self.is(',') {
                            self.next();
                            let (f, _) = self.ident()?;
                            fields.push(f);
                        }
                        self.symbol(']')?;
                        self.symbol(')')?;
                        if fields.is_empty() { return Err(err(&first_tok, "@@index requires at least one field")); }
                        let field = fields[0].clone();
                        model.indexes.push(Index {
                            field,
                            fields,
                            line: first_tok.line,
                            column: first_tok.col,
                        });
                    }
                    "unique" => {
                        self.symbol('(')?;
                        self.symbol('[')?;
                        let mut fields = Vec::new();
                        let (field, first_tok) = self.ident()?;
                        fields.push(field);
                        while self.is(',') {
                            self.next();
                            let (f, _) = self.ident()?;
                            fields.push(f);
                        }
                        self.symbol(']')?;
                        self.symbol(')')?;
                        if fields.is_empty() { return Err(err(&first_tok, "@@unique requires at least one field")); }
                        model.unique_constraints.push(UniqueConstraint { fields, line: first_tok.line, column: first_tok.col });
                    }
                    _ => return Err(err(&tok, format!("unsupported model attribute '@@{attr}'"))),
                }
                self.end_line()?;
                continue;
            }
            let (field_name, tok) = self.ident()?;
            let (type_name, type_tok) = self.ident()?;
            // Reject Type[]? per spec §5.2: optional list is not allowed
            if self.is('[') {
                // peek ahead: list type
                let saved = self.at;
                self.next(); // [
                if self.is(']') {
                    self.next();
                    if self.is('?') {
                        return Err(err(&type_tok, "Type[]? is not allowed; use Type[] or Type?"));
                    }
                    self.at = saved;
                } else {
                    self.at = saved;
                }
            }
            let scalar_ty = match type_name.as_str() {
                "Int" => Some(Scalar::Int),
                "BigInt" => Some(Scalar::BigInt),
                "Float" => Some(Scalar::Float),
                "Boolean" => Some(Scalar::Boolean),
                "String" => Some(Scalar::String),
                "DateTime" => Some(Scalar::DateTime),
                "Bytes" => Some(Scalar::Bytes),
                "Json" => Some(Scalar::Json),
                "Decimal" => Some(Scalar::Decimal { precision: None, scale: None }),
                _ => None,
            };
            // Enum types will be validated post-parse; for now treat known scalars as scalar, others as relation candidate
            if scalar_ty.is_none() && !["Int", "BigInt", "Float", "Boolean", "String", "DateTime", "Bytes", "Json", "Decimal"].contains(&type_name.as_str()) {
                let mut relation = Relation {
                    name: field_name,
                    target: type_name,
                    relation_name: None,
                    list: false,
                    nullable: false,
                    foreign_key: None,
                    line: type_tok.line,
                    column: type_tok.col,
                };
                if self.is('?') {
                    self.next();
                    relation.nullable = true;
                } else if self.is('[') {
                    self.next();
                    self.symbol(']')?;
                    relation.list = true;
                }
                if self.is_single_at() {
                    self.next();
                    let (attribute, attr_tok) = self.ident()?;
                    if attribute != "relation" {
                        return Err(err(&attr_tok, "unsupported relation attribute"));
                    }
                    (relation.relation_name, relation.foreign_key) = self.relation_arguments()?;
                }
                model.relations.push(relation);
                self.end_line()?;
                continue;
            }
            let ty = scalar_ty.unwrap();
            let nullable = if self.is('?') {
                self.next();
                true
            } else {
                false
            };
            let mut field = Field {
                column: field_name.clone(),
                name: field_name,
                ty,
                nullable,
                id: false,
                unique: false,
                default: None,
                native_type: None,
            };
            let mut attrs = HashSet::new();
            while self.is_single_at() {
                self.next();
                let (attr, a_tok) = self.ident()?;
                if !attrs.insert(attr.clone()) {
                    return Err(err(&a_tok, format!("duplicate @{attr}")));
                }
                match attr.as_str() {
                    "id" => field.id = true,
                    "unique" => field.unique = true,
                    "map" => {
                        self.symbol('(')?;
                        field.column = self.string()?;
                        self.symbol(')')?;
                    }
                    "default" => {
                        self.symbol('(')?;
                        let val = self.next();
                        field.default = Some(match &val.kind {
                            Kind::String(v) => DefaultValue::String(v.clone()),
                            Kind::Number(v) => DefaultValue::Number(v.clone()),
                            Kind::Ident(v) if v == "true" || v == "false" => {
                                DefaultValue::Boolean(v == "true")
                            }
                            Kind::Ident(v) if v == "autoincrement" => {
                                self.symbol('(')?;
                                self.symbol(')')?;
                                DefaultValue::Autoincrement
                            }
                            Kind::Ident(v) if v == "now" => {
                                self.symbol('(')?;
                                self.symbol(')')?;
                                DefaultValue::Now
                            }
                            _ => return Err(err(&val, "unsupported default value")),
                        });
                        self.symbol(')')?;
                    }
                    "db" => {
                        self.symbol('.')?;
                        let (db_ty, _) = self.ident()?;
                        match db_ty.as_str() {
                            "Decimal" | "Numeric" => {
                                let mut precision = None;
                                let mut scale = None;
                                if self.is('(') {
                                    self.next();
                                    let tok = self.next();
                                    let p_str = match &tok.kind {
                                        Kind::Number(n) => n.clone(),
                                        Kind::Ident(s) => s.clone(),
                                        _ => return Err(err(&tok, "expected precision number")),
                                    };
                                    let p_val = p_str.parse::<u32>().map_err(|_| err(&tok, "invalid precision"))?;
                                    precision = Some(p_val);
                                    if self.is(',') {
                                        self.next();
                                        let tok2 = self.next();
                                        let s_str = match &tok2.kind {
                                            Kind::Number(n) => n.clone(),
                                            Kind::Ident(s) => s.clone(),
                                            _ => return Err(err(&tok2, "expected scale")),
                                        };
                                        let s_val = s_str.parse::<u32>().map_err(|_| err(&tok2, "invalid scale"))?;
                                        scale = Some(s_val);
                                    }
                                    self.symbol(')')?;
                                }
                                if let (Some(p), Some(s)) = (precision, scale) {
                                    if s > p { return Err(err(&a_tok, "scale cannot exceed precision")); }
                                }
                                field.ty = Scalar::Decimal { precision, scale };
                                field.native_type = Some(format!("Decimal({},{})", precision.map(|v| v.to_string()).unwrap_or_default(), scale.map(|v| v.to_string()).unwrap_or_default()));
                            }
                            _ => return Err(err(&a_tok, format!("unsupported @db type '{}'", db_ty))),
                        }
                    }
                    _ => {
                        return Err(err(
                            &a_tok,
                            format!("unsupported field attribute '@{attr}'"),
                        ))
                    }
                }
            }
            if field.column.is_empty() || field.column.contains('\0') {
                return Err(err(&tok, "column name must be nonempty and contain no NUL"));
            }
            if field.id && field.nullable {
                return Err(err(&tok, "id cannot be nullable"));
            }
            if let Some(default) = &field.default {
                let valid = match (&field.ty, default) {
                    (Scalar::Int | Scalar::BigInt, DefaultValue::Autoincrement) => field.id,
                    (Scalar::DateTime, DefaultValue::Now) => true,
                    (Scalar::String, DefaultValue::String(_))
                    | (Scalar::Boolean, DefaultValue::Boolean(_)) => true,
                    (Scalar::Int, DefaultValue::Number(n)) => n.parse::<i32>().is_ok(),
                    (Scalar::BigInt, DefaultValue::Number(n)) => n.parse::<i64>().is_ok(),
                    (Scalar::Float, DefaultValue::Number(n)) => {
                        n.parse::<f64>().is_ok_and(|v| v.is_finite())
                    }
                    (Scalar::Decimal { .. }, DefaultValue::Number(n)) => n.parse::<f64>().is_ok_and(|v| v.is_finite()),
                    _ => false,
                };
                if !valid {
                    return Err(err(&tok, "default does not match field type"));
                }
            }
            model.fields.push(field);
            self.end_line()?;
        }
        self.symbol('}')?;
        if model.table.is_empty() || model.table.contains('\0') {
            return Err(err(
                &name_token,
                "table name must be nonempty and contain no NUL",
            ));
        }
        if model.fields.is_empty() {
            return Err(err(&name_token, "model must contain fields"));
        }
        if let Err(message) = validate_primary_key(&model) {
            return Err(err(&name_token, message));
        }
        Ok(model)
    }
}

impl Parser {
    fn parse_enum(&mut self) -> Result<Enum, Diagnostic> {
        let (name, name_tok) = self.ident()?;
        if name.is_empty() || name.contains('\0') { return Err(err(&name_tok, "invalid enum name")); }
        self.symbol('{')?;
        self.lines();
        let mut variants = Vec::new();
        let mut seen = HashSet::new();
        while !self.is('}') {
            if self.peek().kind == Kind::Eof { return Err(err(self.peek(), "unterminated enum")); }
            if self.is('@') {
                self.symbol('@')?; self.symbol('@')?;
                let (attr, tok) = self.ident()?;
                if attr != "map" { return Err(err(&tok, format!("unsupported enum block attribute '@@{attr}'"))); }
                // @@map not allowed on enum block currently, just consume
                self.symbol('(')?; let _ = self.string()?; self.symbol(')')?;
                self.end_line()?; continue;
            }
            let (var_name, var_tok) = self.ident()?;
            if !seen.insert(var_name.clone()) { return Err(err(&var_tok, format!("duplicate enum variant '{var_name}'"))); }
            let mut mapped = None;
            if self.is('@') {
                self.next();
                let (attr, a_tok) = self.ident()?;
                if attr != "map" { return Err(err(&a_tok, "only @map is allowed on enum variants")); }
                self.symbol('(')?;
                mapped = Some(self.string()?);
                self.symbol(')')?;
            }
            variants.push(EnumVariant { name: var_name, mapped });
            self.end_line()?;
        }
        self.symbol('}')?;
        if variants.is_empty() { return Err(err(&name_tok, "enum must have at least one variant")); }
        Ok(Enum { name, variants })
    }
    fn view(&mut self) -> Result<View, Diagnostic> {
        let (name, name_tok) = self.ident()?;
        if name.is_empty() || name.contains('\0') { return Err(err(&name_tok, "invalid view name")); }
        self.symbol('{')?; self.lines();
        let mut view = View { name: name.clone(), table: name.clone(), fields: vec![] };
        let mut has_map = false;
        while !self.is('}') {
            if self.peek().kind == Kind::Eof { return Err(err(self.peek(), "unterminated view")); }
            if self.is('@') {
                self.symbol('@')?; self.symbol('@')?;
                let (attr, tok) = self.ident()?;
                match attr.as_str() {
                    "map" => {
                        if has_map { return Err(err(&tok, "duplicate @@map")); }
                        has_map = true;
                        self.symbol('(')?; view.table = self.string()?; self.symbol(')')?;
                    }
                    _ => return Err(err(&tok, format!("unsupported view attribute '@@{attr}'"))),
                }
                self.end_line()?; continue;
            }
            let (field_name, _) = self.ident()?;
            let (type_name, _) = self.ident()?;
            let scalar_ty = match type_name.as_str() {
                "Int" => Some(Scalar::Int), "BigInt" => Some(Scalar::BigInt), "Float" => Some(Scalar::Float),
                "Boolean" => Some(Scalar::Boolean), "String" => Some(Scalar::String), "DateTime" => Some(Scalar::DateTime),
                "Bytes" => Some(Scalar::Bytes), "Json" => Some(Scalar::Json), "Decimal" => Some(Scalar::Decimal{precision: None, scale: None}),
                _ => None,
            };
            let ty = scalar_ty.ok_or_else(|| err(self.peek(), format!("unsupported view field type '{type_name}'")))?;
            let nullable = if self.is('?') { self.next(); true } else { false };
            let mut field = Field { column: field_name.clone(), name: field_name, ty, nullable, id: false, unique: false, default: None, native_type: None };
            while self.is_single_at() {
                self.next();
                let (attr, _) = self.ident()?;
                match attr.as_str() {
                    "map" => { self.symbol('(')?; field.column = self.string()?; self.symbol(')')?; },
                    "id" => field.id = true,
                    "unique" => field.unique = true,
                    _ => return Err(err(self.peek(), format!("unsupported view field attribute '@{attr}'"))),
                }
            }
            view.fields.push(field);
            self.end_line()?;
        }
        self.symbol('}')?;
        if view.fields.is_empty() { return Err(err(&name_tok, "view must contain fields")); }
        Ok(view)
    }
    fn composite(&mut self) -> Result<CompositeType, Diagnostic> {
        let (name, name_tok) = self.ident()?;
        if name.is_empty() || name.contains('\0') { return Err(err(&name_tok, "invalid composite type name")); }
        self.symbol('{')?; self.lines();
        let mut comp = CompositeType { name, fields: vec![] };
        while !self.is('}') {
            if self.peek().kind == Kind::Eof { return Err(err(self.peek(), "unterminated composite type")); }
            let (field_name, _) = self.ident()?;
            let (type_name, _) = self.ident()?;
            let scalar_ty = match type_name.as_str() {
                "Int" => Some(Scalar::Int), "BigInt" => Some(Scalar::BigInt), "Float" => Some(Scalar::Float),
                "Boolean" => Some(Scalar::Boolean), "String" => Some(Scalar::String), "DateTime" => Some(Scalar::DateTime),
                "Bytes" => Some(Scalar::Bytes), "Json" => Some(Scalar::Json), "Decimal" => Some(Scalar::Decimal{precision: None, scale: None}),
                _ => None,
            };
            let ty = scalar_ty.ok_or_else(|| err(self.peek(), format!("unsupported composite field type '{type_name}'")))?;
            let mut field = Field { column: field_name.clone(), name: field_name, ty, nullable: false, id: false, unique: false, default: None, native_type: None };
            if self.is('?') { self.next(); field.nullable = true; }
            comp.fields.push(field);
            self.end_line()?;
        }
        self.symbol('}')?;
        if comp.fields.is_empty() { return Err(err(&name_tok, "composite type must have fields")); }
        Ok(comp)
    }
}
fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
fn rust_ident(s: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while", "gen", "abstract", "become", "box",
        "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    if KEYWORDS.contains(&s) {
        format!("r#{s}")
    } else {
        s.to_owned()
    }
}
/// Parse and validate the explicitly supported schema subset. Unknown syntax fails closed.
pub fn parse(input: &str) -> Result<Schema, Diagnostic> {
    let mut p = Parser {
        tokens: lex(input)?,
        at: 0,
    };
    let mut models = vec![];
    let mut views: Vec<View> = vec![];
    let mut composites: Vec<CompositeType> = vec![];
    let mut enums = vec![];
    let mut ds = false;
    let mut gen = false;
    let mut output = None;
    p.lines();
    while p.peek().kind != Kind::Eof {
        let (block, tok) = p.ident()?;
        match block.as_str() {
            "datasource" if !ds => {
                ds = true;
                p.config("datasource")?;
            }
            "generator" if !gen => {
                gen = true;
                output = p.config("generator")?;
            }
            "model" => models.push(p.model()?),
            "view" => views.push(p.view()?),
            "type" => composites.push(p.composite()?),
            "enum" => enums.push(p.parse_enum()?),
            "datasource" | "generator" => {
                return Err(err(&tok, format!("duplicate {block} block")))
            }
            _ => return Err(err(&tok, format!("unsupported block '{block}'"))),
        }
        p.end_line()?;
    }
    if !ds {
        return Err(err(p.peek(), "missing datasource block"));
    }
    if models.is_empty() && views.is_empty() && composites.is_empty() && enums.is_empty() {
        return Err(err(p.peek(), "schema must contain a model, view, type or enum"));
    }
    let mut names = HashSet::new();
    let mut tables = HashSet::new();
    let mut modules = HashSet::new();
    for m in &models {
        if !names.insert(&m.name) || !tables.insert(&m.table) || !modules.insert(snake(&m.name)) {
            return Err(err(
                p.peek(),
                format!(
                    "duplicate model, table or generated module name for '{}'",
                    m.name
                ),
            ));
        }
        // also ensure enum modules don't collide
        if enums.iter().any(|e| snake(&e.name) == snake(&m.name)) { return Err(err(p.peek(), format!("model/enum module collision for '{}'", m.name))); }
        let mut fields = HashSet::new();
        let mut columns = HashSet::new();
        let mut generated = HashSet::new();
        for f in &m.fields {
            if !fields.insert(&f.name)
                || !columns.insert(&f.column)
                || !generated.insert(snake(&f.name))
            {
                return Err(err(
                    p.peek(),
                    format!("duplicate field, column or generated name in '{}'", m.name),
                ));
            }
            // These keywords cannot be used as raw identifiers in Rust.
            if ["self", "super", "crate", "Self", "_"].contains(&snake(&f.name).as_str()) {
                return Err(err(
                    p.peek(),
                    format!("field '{}' cannot be represented in Rust", f.name),
                ));
            }
        }
        for relation in &m.relations {
            if [
                "columns",
                "find_many",
                "create",
                "count",
                "update_many",
                "delete_many",
            ]
            .contains(&snake(&relation.name).as_str())
            {
                return Err(Diagnostic {
                    line: relation.line,
                    column: relation.column,
                    message: format!(
                        "relation '{}' conflicts with a generated method",
                        relation.name
                    ),
                });
            }
            if !fields.insert(&relation.name) || !generated.insert(snake(&relation.name)) {
                return Err(Diagnostic {
                    line: relation.line,
                    column: relation.column,
                    message: format!("duplicate field or generated name in '{}'", m.name),
                });
            }
            if ["self", "super", "crate", "Self", "_"].contains(&snake(&relation.name).as_str()) {
                return Err(Diagnostic {
                    line: relation.line,
                    column: relation.column,
                    message: format!("relation '{}' cannot be represented in Rust", relation.name),
                });
            }
        }
        if ["self", "super", "crate", "Self", "_"].contains(&snake(&m.name).as_str()) {
            return Err(err(
                p.peek(),
                format!("model '{}' cannot be represented in Rust", m.name),
            ));
        }
    }
    // Reject DEFERRABLE/INITIALLY per §9/§10 R2 (SET DEFAULT handled via relation onDelete)
    {
        let lower = input.to_ascii_lowercase();
        if lower.contains("deferrable") || lower.contains("initially") {
            return Err(Diagnostic { line: 1, column: 1, message: "DEFERRABLE/INITIALLY is not supported; relations must be immediate".into() });
        }
    }
    // Detect enum vs model name collisions and validate enums
    let mut enum_names = HashSet::new();
    for e in &enums {
        if !enum_names.insert(&e.name) { return Err(err(p.peek(), format!("duplicate enum or model name '{}'", e.name))); }
        if models.iter().any(|m| &m.name == &e.name) { return Err(err(p.peek(), format!("duplicate model/enum name '{}'", e.name))); }
    }
    // After full parse, reclassify unknown types that are actually enum names: treat as scalar enum, not relation
    // For fields that were parsed as relations but target is enum, convert to scalar Field
    let mut fixed_models = Vec::new();
    for mut m in models {
        let mut new_relations = Vec::new();
        let mut extra_fields = Vec::new();
        for r in m.relations {
            if enum_names.contains(&r.target) {
                // enum field: must be non-list (P0 enums are not lists)
                if r.list { return Err(Diagnostic { line: r.line, column: r.column, message: format!("enum field '{}' cannot be a list", r.name)}); }
                if r.foreign_key.is_some() || r.relation_name.is_some() { return Err(Diagnostic { line: r.line, column: r.column, message: format!("enum field '{}' cannot have @relation", r.name)}); }
                extra_fields.push(Field { name: r.name.clone(), column: r.name.clone(), ty: Scalar::Enum(r.target.clone()), nullable: r.nullable, id: false, unique: false, default: None, native_type: None });
            } else {
                new_relations.push(r);
            }
        }
        m.relations = new_relations;
        m.fields.extend(extra_fields);
        fixed_models.push(m);
    }
    let schema = Schema {
        models: fixed_models,
        views: views.clone(),
        composites: composites.clone(),
        enums,
        generator_output: output,
    };
    validate_indexes(&schema)?;
    validate_relations(&schema)?;
    validate_enums(&schema)?;
    Ok(schema)
}
/// Validate scalar indexes (single and composite) and @@unique constraints.
pub fn validate_indexes(schema: &Schema) -> Result<(), Diagnostic> {
    for model in &schema.models {
        // collect FK fields for this model to reject composite indexes that include a FK (P0 compat: relations.rs expects @@index([authorId, title]) to be invalid while p0 expects @@index([a,b]) valid for non-FK)
        let fk_fields: std::collections::HashSet<String> = model.relations.iter().filter_map(|r| r.foreign_key.as_ref().map(|fk| fk.field.clone())).flat_map(|s| s.split(',').map(|x| x.to_string()).collect::<Vec<_>>()).collect();
        let mut seen = HashSet::new();
        for index in &model.indexes {
            if index.fields.is_empty() { return Err(Diagnostic { line: index.line, column: index.column, message: format!("empty @@index in '{}'", model.name)}); }
            // forged-index check: `field` must match `fields[0]` and be a real scalar field (relations.rs: modifies `field` to "nonexistent" while `fields` stays valid)
            if index.field != index.fields[0] {
                return Err(Diagnostic { line: index.line, column: index.column, message: format!("invalid index field '{}' in '{}'", index.field, model.name)});
            }
            if model.fields.iter().filter(|field| field.name == index.field).count() != 1 {
                return Err(Diagnostic { line: index.line, column: index.column, message: format!("invalid index field '{}' in '{}'", index.field, model.name)});
            }
            let mut dup = HashSet::new();
            for f in &index.fields {
                if !dup.insert(f) { return Err(Diagnostic { line: index.line, column: index.column, message: format!("duplicate field '{}' in @@index '{}'", f, model.name)}); }
                if model.fields.iter().filter(|field| field.name == *f).count() != 1 {
                    return Err(Diagnostic { line: index.line, column: index.column, message: format!("invalid index field '{}' in '{}'", f, model.name)});
                }
            }
            // composite index containing a FK field is not supported (keeps relations.rs passing while p0 non-FK composite stays valid)
            if index.fields.len() > 1 && index.fields.iter().any(|f| fk_fields.contains(f)) {
                return Err(Diagnostic { line: index.line, column: index.column, message: format!("composite @@index cannot include foreign key field in '{}'", model.name)});
            }
            // prevent exact duplicate index definitions
            let key = index.fields.join(",");
            if !seen.insert(key) { return Err(Diagnostic { line: index.line, column: index.column, message: format!("duplicate @@index in '{}'", model.name)}); }
        }
        let mut seen_u = HashSet::new();
        for uc in &model.unique_constraints {
            if uc.fields.is_empty() || uc.fields.len() < 2 { return Err(Diagnostic { line: uc.line, column: uc.column, message: format!("@@unique requires at least two fields in '{}'", model.name)}); }
            let mut dup = HashSet::new();
            for f in &uc.fields {
                if !dup.insert(f) { return Err(Diagnostic { line: uc.line, column: uc.column, message: format!("duplicate field '{}' in @@unique '{}'", f, model.name)}); }
                if model.fields.iter().filter(|field| field.name == *f).count() != 1 {
                    return Err(Diagnostic { line: uc.line, column: uc.column, message: format!("invalid @@unique field '{}' in '{}'", f, model.name)});
                }
            }
            let key = uc.fields.join(",");
            if !seen_u.insert(key.clone()) { return Err(Diagnostic { line: uc.line, column: uc.column, message: format!("duplicate @@unique in '{}'", model.name)}); }
            // @@unique must not duplicate @@id
            if model.primary_key.len() >= 2 && model.primary_key.join(",") == key { return Err(Diagnostic { line: uc.line, column: uc.column, message: format!("@@unique duplicates @@id in '{}'", model.name)}); }
        }
    }
    Ok(())
}
/// Validate scalar or composite primary keys, including schemas constructed in code.
fn validate_primary_key(model: &Model) -> Result<(), &'static str> {
    let scalar_ids = model.fields.iter().filter(|field| field.id).count();
    if model.primary_key.is_empty() {
        if scalar_ids != 1 || model.fields.iter().any(|field| field.id && field.nullable) {
            return Err("model must have exactly one non-null scalar @id or one composite @@id");
        }
        return Ok(());
    }
    if scalar_ids != 0 || model.primary_key.len() < 2 || model.primary_key.len() > 32 {
        return Err("@@id needs between two and 32 fields and cannot be combined with @id");
    }
    let mut seen = HashSet::new();
    for key in &model.primary_key {
        let fields: Vec<_> = model.fields.iter().filter(|f| f.name == *key).collect();
        if !seen.insert(key) || fields.len() != 1 || fields[0].nullable {
            return Err("@@id fields must be distinct, existing, non-null scalar fields");
        }
    }
    Ok(())
}
pub fn validate_enums(schema: &Schema) -> Result<(), Diagnostic> {
    for e in &schema.enums {
        if e.variants.is_empty() { return Err(Diagnostic { line: 0, column: 0, message: format!("enum '{}' must have variants", e.name)}); }
        let mut seen = std::collections::HashSet::new();
        for v in &e.variants { if !seen.insert(&v.name) { return Err(Diagnostic { line: 0, column: 0, message: format!("duplicate variant '{}' in enum '{}'", v.name, e.name)}); } }
    }
    Ok(())
}
/// Revalidate primary keys before generating SQL from an externally constructed schema.
pub fn validate_primary_keys(schema: &Schema) -> Result<(), Diagnostic> {
    for model in &schema.models {
        validate_primary_key(model).map_err(|message| Diagnostic {
            line: 0,
            column: 0,
            message: format!("{}: {message}", model.name),
        })?;
    }
    Ok(())
}
/// Find the paired end, without guessing between equally named relations.
pub(crate) fn inverse_relation<'a>(
    model: &Model,
    relation: &Relation,
    target: &'a Model,
) -> Option<&'a Relation> {
    let mut matches = target.relations.iter().filter(|r| {
        r.target == model.name
            && r.relation_name == relation.relation_name
            && (model.name != target.name || r.name != relation.name)
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}
/// Validate single-column FK relations and unnamed, non-self implicit N–N.
/// This is also called on externally constructed schemas before SQL generation.
pub fn validate_relations(schema: &Schema) -> Result<(), Diagnostic> {
    for model in &schema.models {
        let mut used_fk = HashSet::new();
        for relation in &model.relations {
            let fail = |message: &str| Diagnostic {
                line: relation.line,
                column: relation.column,
                message: message.into(),
            };
            if relation.name.is_empty() || relation.target.is_empty() {
                return Err(fail("empty relation name or target"));
            }
            let targets: Vec<_> = schema
                .models
                .iter()
                .filter(|m| m.name == relation.target)
                .collect();
            if targets.len() != 1 {
                return Err(fail(&format!(
                    "unsupported field type '{}' or relation target is ambiguous",
                    relation.target
                )));
            }
            let target = targets[0];
            if relation
                .relation_name
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.contains('\0'))
            {
                return Err(fail("invalid relation name"));
            }
            let is_self = target.name == model.name;
            if is_self && relation.relation_name.is_none() {
                return Err(fail("self relations require an explicit name"));
            }
            let other = inverse_relation(model, relation, target).ok_or_else(|| {
                fail("relation requires one unambiguous inverse with the same name")
            })?;
            let expected = if is_self { 2 } else { 1 };
            if model
                .relations
                .iter()
                .filter(|r| r.target == target.name && r.relation_name == relation.relation_name)
                .count()
                != expected
            {
                return Err(fail(
                    "relation requires one unambiguous inverse with the same name",
                ));
            }
            if relation.list {
                if relation.nullable || relation.foreign_key.is_some() {
                    return Err(fail(
                        "list relations cannot own a foreign key or be nullable",
                    ));
                }
                if other.list {
                    // P1: named and self M2M allowed via explicit through or named implicit
                    if other.foreign_key.is_some() || other.nullable {
                        return Err(fail("invalid implicit many-to-many inverse"));
                    }
                    let id = model
                        .fields
                        .iter()
                        .find(|f| f.id)
                        .ok_or_else(|| fail("implicit many-to-many requires a primary key"))?;
                    if id.nullable || matches!(id.ty, Scalar::Float | Scalar::Decimal { .. }) {
                        return Err(fail("unsupported implicit many-to-many primary key"));
                    }
                } else if other.foreign_key.is_none() {
                    return Err(fail("list inverse requires a foreign key"));
                }
                continue;
            }
            if relation.foreign_key.is_none() {
                if !relation.nullable || other.list || other.foreign_key.is_none() {
                    return Err(fail(
                        "inverse one-to-one must be optional and have one owning side",
                    ));
                }
                continue;
            }
            let fk = relation.foreign_key.as_ref().unwrap();
            if other.foreign_key.is_some() || (!other.list && !other.nullable) {
                return Err(fail(
                    "relation must have exactly one owner and an optional singular inverse",
                ));
            }
            if !used_fk.insert(&fk.field) {
                return Err(fail("foreign key field is used by multiple relations"));
            }
            // P1: handle composite FK (comma-separated)
            let fk_fields: Vec<&str> = fk.field.split(',').collect();
            let fk_refs: Vec<&str> = fk.references.split(',').collect();
            if fk_fields.len() != fk_refs.len() {
                return Err(fail("composite foreign key fields and references must have same count"));
            }
            // duplicate field/reference in composite FK is invalid (relations.rs expects `fields: [authorId, id], references: [id, id]` to fail)
            {
                let mut seen_f = std::collections::HashSet::new();
                for f in &fk_fields { if !seen_f.insert(*f) { return Err(fail("duplicate field in composite foreign key")); } }
                let mut seen_r = std::collections::HashSet::new();
                for r in &fk_refs { if !seen_r.insert(*r) { return Err(fail("duplicate reference in composite foreign key")); } }
            }
            let is_full_composite_pk = !target.primary_key.is_empty() && fk_refs.len() == target.primary_key.len() && fk_refs.iter().zip(target.primary_key.iter()).all(|(r, pk)| *r == pk.as_str());
            for (lf, rf) in fk_fields.iter().zip(fk_refs.iter()) {
                let local: Vec<_> = model.fields.iter().filter(|f| &f.name == lf).collect();
                let remote: Vec<_> = target.fields.iter().filter(|f| &f.name == rf).collect();
                if local.len() != 1 || remote.len() != 1 {
                    return Err(fail("foreign key fields must exist and be unambiguous"));
                }
                if matches!(local[0].ty, Scalar::Float | Scalar::Decimal { .. }) {
                    return Err(fail("Float/Decimal relation keys are not supported"));
                }
                let references_composite_pk = is_full_composite_pk && target.primary_key.iter().any(|k| k == rf);
                if local[0].ty != remote[0].ty || !(remote[0].id || remote[0].unique || references_composite_pk) || remote[0].nullable {
                    return Err(fail("foreign key must reference a compatible non-null unique field"));
                }
                if relation.nullable != local[0].nullable {
                    return Err(fail("relation and foreign key nullability must match"));
                }
            }
            // Keep legacy single-field variables for later checks
            let local: Vec<_> = model.fields.iter().filter(|f| f.name == fk_fields[0].to_string()).collect();
            let remote: Vec<_> = target.fields.iter().filter(|f| f.name == fk_refs[0].to_string()).collect();
            if local.len() != 1 || remote.len() != 1 {
                return Err(fail("foreign key fields must exist and be unambiguous"));
            }
            if other.list && (local[0].id || local[0].unique) {
                return Err(fail("one-to-many foreign key cannot be unique"));
            }
            if !other.list && !(local[0].id || local[0].unique) {
                return Err(fail("one-to-one foreign key must be unique"));
            }
            if !local[0].nullable
                && (fk.on_delete == ReferentialAction::SetNull
                    || fk.on_update == ReferentialAction::SetNull)
            {
                return Err(fail("SetNull requires a nullable foreign key"));
            }
        }
    }
    Ok(())
}
/// Stable internal table name for a non-self implicit many-to-many relation.
/// Its orientation is lexicographic by logical model name, independent of schema order.
pub fn implicit_join_name(left: &str, right: &str) -> String {
    let (a, b) = if left < right {
        (left, right)
    } else {
        (right, left)
    };
    let digest = crate::checksum::sha256(format!("{a}\0{b}").as_bytes());
    format!("rustorm_m2m_{}", &digest[..32])
}
/// P1: named and self M2M join name (uses relation name if provided, otherwise left/right)
pub fn implicit_join_name_for_relation(relation_name: Option<&str>, left: &str, right: &str) -> String {
    if let Some(name) = relation_name {
        let digest = crate::checksum::sha256(format!("{}\0{}\0{}", name, left, right).as_bytes());
        format!("rustorm_m2m_{}", &digest[..32])
    } else {
        implicit_join_name(left, right)
    }
}
fn ty(f: &Field) -> String {
    let base = match &f.ty {
        Scalar::Int => "i32".to_string(),
        Scalar::BigInt => "i64".to_string(),
        Scalar::Float => "f64".to_string(),
        Scalar::Boolean => "bool".to_string(),
        Scalar::String => "String".to_string(),
        Scalar::DateTime => "String".to_string(),
        Scalar::Bytes => "Vec<u8>".to_string(),
        Scalar::Json => "String".to_string(),
        Scalar::Decimal { .. } => "String".to_string(),
        Scalar::Enum(name) => name.clone(),
    };
    if f.nullable {
        format!("Option<{base}>")
    } else {
        base
    }
}
fn generate_unique_child_fields(out: &mut String, model: &Model) {
    out.push_str(".with_unique_child_fields(&[");
    for field in model.fields.iter().filter(|f| f.id || f.unique) {
        out.push_str(&format!("{:?}, ", field.column));
    }
    out.push_str("])");
    if model.primary_key.len() >= 2 {
        out.push_str(".with_composite_child_key(&[");
        for name in &model.primary_key {
            if let Some(field) = model.fields.iter().find(|f| &f.name == name) {
                out.push_str(&format!("{:?}, ", field.column));
            }
        }
        out.push_str("])");
    }
}
/// Generate a Rust module. The output is deterministic and contains no credentials.
/// Format a schema file deterministically. Returns formatted string or diagnostic on invalid input.
/// Idempotent: format(format(input)) == format(input).
pub fn format(input: &str) -> Result<String, Diagnostic> {
    let schema = parse(input)?;
    // Reconstruct from IR with canonical formatting
    let mut out = String::new();
    // Find datasource/generator raw? Re-emit canonical
    out.push_str("datasource db {\n  provider = \"postgresql\"\n  url = env(\"DATABASE_URL\")\n}\n\n");
    if let Some(dir) = &schema.generator_output {
        out.push_str(&format!("generator client {{\n  provider = \"rustorm-client\"\n  output = \"{dir}\"\n}}\n\n"));
    } else {
        out.push_str("generator client {\n  provider = \"rustorm-client\"\n  output = \"../src/generated/rustorm\"\n}\n\n");
    }
    for e in &schema.enums {
        out.push_str(&format!("enum {} {{\n", e.name));
        for v in &e.variants {
            if let Some(m) = &v.mapped { out.push_str(&format!("  {} @map(\"{m}\")\n", v.name)); } else { out.push_str(&format!("  {}\n", v.name)); }
        }
        out.push_str("}\n\n");
    }
    for m in &schema.models {
        out.push_str(&format!("model {} {{\n", m.name));
        for f in &m.fields {
            let ty_str = match &f.ty {
                Scalar::Int => "Int",
                Scalar::BigInt => "BigInt",
                Scalar::Float => "Float",
                Scalar::Boolean => "Boolean",
                Scalar::String => "String",
                Scalar::DateTime => "DateTime",
                Scalar::Bytes => "Bytes",
                Scalar::Json => "Json",
                Scalar::Decimal { .. } => "Decimal",
                Scalar::Enum(n) => n.as_str(),
            };
            let mut line = format!("  {} {}{}", f.name, ty_str, if f.nullable { "?" } else { "" });
            if f.id { line.push_str(" @id"); }
            if f.unique { line.push_str(" @unique"); }
            if let Some(mapped) = (f.column != f.name).then_some(&f.column) { line.push_str(&format!(" @map(\"{mapped}\")")); }
            if let Some(def) = &f.default { match def { DefaultValue::Autoincrement => line.push_str(" @default(autoincrement())"), DefaultValue::Now => line.push_str(" @default(now())"), DefaultValue::String(s) => line.push_str(&format!(" @default(\"{s}\")")), DefaultValue::Number(n) => line.push_str(&format!(" @default({n})")), DefaultValue::Boolean(b) => line.push_str(&format!(" @default({})", if *b { "true" } else { "false" })), } }
            if let Scalar::Decimal { precision, scale } = &f.ty {
                if precision.is_some() || scale.is_some() {
                    line.push_str(&format!(" @db.Decimal({},{})", precision.unwrap_or(0), scale.unwrap_or(0)));
                } else if f.native_type.is_some() {
                    // Fallback if native_type set via @db
                    line.push_str(&format!(" @db.{}", f.native_type.as_ref().unwrap()));
                }
            }
            out.push_str(&line); out.push_str("\n");
        }
        for r in &m.relations {
            let mut line = format!("  {} {}", r.name, r.target);
            if r.list { line.push_str("[]"); } else if r.nullable { line.push_str("?"); }
            if r.relation_name.is_some() || r.foreign_key.is_some() {
                line.push_str(" @relation(");
                let mut parts = Vec::new();
                if let Some(name) = &r.relation_name { parts.push(format!("\"{name}\"")); }
                if let Some(fk) = &r.foreign_key {
                    if r.relation_name.is_some() { parts.push(format!("fields: [{}]", fk.field)); } else { parts.push(format!("fields: [{}]", fk.field)); }
                    parts.push(format!("references: [{}]", fk.references));
                    parts.push(format!("onDelete: {}", match fk.on_delete { ReferentialAction::Cascade=>"Cascade", ReferentialAction::Restrict=>"Restrict", ReferentialAction::NoAction=>"NoAction", ReferentialAction::SetNull=>"SetNull" }));
                    parts.push(format!("onUpdate: {}", match fk.on_update { ReferentialAction::Cascade=>"Cascade", ReferentialAction::Restrict=>"Restrict", ReferentialAction::NoAction=>"NoAction", ReferentialAction::SetNull=>"SetNull" }));
                }
                line.push_str(&parts.join(", "));
                line.push_str(")");
            }
            out.push_str(&line); out.push_str("\n");
        }
        for _pk in &m.primary_key { /* @@id handled as aggregate */ }
        if !m.primary_key.is_empty() { out.push_str(&format!("  @@id([{}])\n", m.primary_key.join(", "))); }
        for uc in &m.unique_constraints { out.push_str(&format!("  @@unique([{}])\n", uc.fields.join(", "))); }
        for idx in &m.indexes { out.push_str(&format!("  @@index([{}])\n", idx.fields.join(", "))); }
        if m.table != m.name { out.push_str(&format!("  @@map(\"{}\")\n", m.table)); }
        out.push_str("}\n\n");
    }
    for v in &schema.views {
        out.push_str(&format!("view {} {{\n", v.name));
        for f in &v.fields {
            let ty_str = match &f.ty {
                Scalar::Int => "Int", Scalar::BigInt => "BigInt", Scalar::Float => "Float", Scalar::Boolean => "Boolean",
                Scalar::String => "String", Scalar::DateTime => "DateTime", Scalar::Bytes => "Bytes", Scalar::Json => "Json",
                Scalar::Decimal { .. } => "Decimal", Scalar::Enum(n) => n.as_str(),
            };
            let mut line = format!("  {} {}{}", f.name, ty_str, if f.nullable { "?" } else { "" });
            if f.id { line.push_str(" @id"); }
            if f.unique { line.push_str(" @unique"); }
            if f.column != f.name { line.push_str(&format!(" @map(\"{}\")", f.column)); }
            out.push_str(&line); out.push_str("\n");
        }
        if v.table != v.name { out.push_str(&format!("  @@map(\"{}\")\n", v.table)); }
        out.push_str("}\n\n");
    }
    for c in &schema.composites {
        out.push_str(&format!("type {} {{\n", c.name));
        for f in &c.fields {
            let ty_str = match &f.ty {
                Scalar::Int => "Int", Scalar::BigInt => "BigInt", Scalar::Float => "Float", Scalar::Boolean => "Boolean",
                Scalar::String => "String", Scalar::DateTime => "DateTime", Scalar::Bytes => "Bytes", Scalar::Json => "Json",
                Scalar::Decimal { .. } => "Decimal", Scalar::Enum(n) => n.as_str(),
            };
            out.push_str(&format!("  {} {}{}\n", f.name, ty_str, if f.nullable { "?" } else { "" }));
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}

pub fn generate(schema: &Schema) -> String {
    let mut out = String::from("// @generated by rustorm; do not edit.\n\n");
    // Generate enums first
    for e in &schema.enums {
        out.push_str(&format!("pub mod {} {{\n", rust_ident(&snake(&e.name))));
        out.push_str("    #[derive(Debug, Clone, PartialEq, Eq)]\n    pub enum Variants {\n");
        for v in &e.variants { out.push_str(&format!("        {},\n", rust_ident(v.name.as_str()))); }
        out.push_str("    }\n}\n\n");
    }
    for m in &schema.models {
        out.push_str(&format!("pub mod {} {{\n", rust_ident(&snake(&m.name))));
        out.push_str("    #[derive(Debug, Clone, PartialEq)]\n    pub struct Record {\n");
        for f in &m.fields {
            out.push_str(&format!(
                "        pub {}: {},\n",
                rust_ident(&snake(&f.name)),
                ty(f)
            ));
        }
        out.push_str("    }\n\n");
        out.push_str("    impl ::rustorm::row::FromRow for Record {\n        fn from_row(row: &::rustorm::row::Row) -> Result<Self, ::rustorm::row::DecodeError> {\n            Ok(Self {\n");
        for f in &m.fields {
            out.push_str(&format!(
                "                {}: row.get({:?})?,\n",
                rust_ident(&snake(&f.name)),
                f.column
            ));
        }
        out.push_str("            })\n        }\n    }\n\n");
        for f in &m.fields {
            out.push_str(&format!("    pub const {}: ::rustorm::query::Field<{}> = ::rustorm::query::Field::new({:?}, {:?});\n", snake(&f.name).to_ascii_uppercase(), ty(f), m.table, f.column));
        }
        out.push_str("\n    pub fn columns() -> &'static [&'static str] { &[");
        for f in &m.fields {
            out.push_str(&format!("{:?}, ", f.column));
        }
        out.push_str("] }\n");
        out.push_str(&format!("\n    pub fn find_many() -> ::rustorm::query::Select {{\n        ::rustorm::query::Select::new({:?}, &[", m.table));
        for f in &m.fields {
            out.push_str(&format!("{:?}, ", f.column));
        }
        out.push_str("])\n    }\n");
        out.push_str(&format!("\n    pub fn count() -> ::rustorm::query::Count {{\n        ::rustorm::query::Count::new({:?})\n    }}\n", m.table));
        out.push_str(&format!("\n    pub fn create() -> ::rustorm::query::Insert {{\n        ::rustorm::query::Insert::new({:?}, &[", m.table));
        for f in &m.fields {
            if !f.nullable && f.default.is_none() {
                out.push_str(&format!("{:?}, ", f.column));
            }
        }
        out.push_str("])\n    }\n");
        out.push_str(&format!("\n    pub fn update_many() -> ::rustorm::query::Update {{\n        ::rustorm::query::Update::new({:?})\n    }}\n", m.table));
        out.push_str(&format!("\n    pub fn delete_many() -> ::rustorm::query::Delete {{\n        ::rustorm::query::Delete::new({:?})\n    }}\n", m.table));
        // P0 findUnique / findFirst helpers
        out.push_str(&format!("\n    pub fn find_unique() -> ::rustorm::query::Select {{\n        ::rustorm::query::Select::new({:?}, &[", m.table));
        for f in &m.fields { out.push_str(&format!("{:?}, ", f.column)); }
        out.push_str("])\n    }\n");
        out.push_str(&format!("\n    pub fn find_first() -> ::rustorm::query::Select {{\n        ::rustorm::query::Select::new({:?}, &[", m.table));
        for f in &m.fields { out.push_str(&format!("{:?}, ", f.column)); }
        out.push_str("])\n    }\n");
        // create_many helper (P1 but provide stub for P0 completeness)
        out.push_str(&format!("\n    pub fn create_many() -> ::rustorm::query::InsertManyStub {{\n        ::rustorm::query::InsertManyStub::new({:?})\n    }}\n", m.table));
        for relation in m.relations.iter().filter(|r| r.foreign_key.is_some()) {
            let fk = relation.foreign_key.as_ref().unwrap();
            let parent = schema
                .models
                .iter()
                .find(|model| model.name == relation.target);
            if let Some((parent, local, remote)) = parent.and_then(|p| {
                Some((
                    p,
                    m.fields.iter().find(|f| f.name == fk.field)?,
                    p.fields.iter().find(|f| f.name == fk.references)?,
                ))
            }) {
                let key_type = match remote.ty {
                    Scalar::Int => "i32",
                    Scalar::BigInt => "i64",
                    Scalar::Boolean => "bool",
                    Scalar::String => "String",
                    Scalar::Float => continue,
                    Scalar::DateTime => continue,
                    Scalar::Bytes => continue,
                    Scalar::Json => continue,
                    Scalar::Decimal { .. } => continue,
                    Scalar::Enum(_) => continue,
                };
                out.push_str(&format!("\n    pub fn {}() -> ::rustorm::relations::One<Record, super::{}::Record, {}> {{\n        ::rustorm::relations::One::new({:?}, {:?}, {:?}, {:?}, {}, &[", rust_ident(&snake(&relation.name)), rust_ident(&snake(&parent.name)), key_type, m.table, local.column, parent.table, remote.column, local.nullable));
                for field in &parent.fields {
                    out.push_str(&format!("{:?}, ", field.column));
                }
                out.push_str("])");
                generate_unique_child_fields(&mut out, m);
                out.push_str("\n    }\n");
            }
        }
        for relation in m
            .relations
            .iter()
            .filter(|r| !r.list && r.foreign_key.is_none())
        {
            if let Some(child) = schema
                .models
                .iter()
                .find(|model| model.name == relation.target)
            {
                if let Some(fk) =
                    inverse_relation(m, relation, child).and_then(|r| r.foreign_key.as_ref())
                {
                    if let (Some(parent_key), Some(child_key)) = (
                        m.fields.iter().find(|f| f.name == fk.references),
                        child.fields.iter().find(|f| f.name == fk.field),
                    ) {
                        let key_type = match parent_key.ty {
                            Scalar::Int => "i32",
                            Scalar::BigInt => "i64",
                            Scalar::Boolean => "bool",
                            Scalar::String => "String",
                            Scalar::Float => continue,
                    Scalar::DateTime => continue,
                    Scalar::Bytes => continue,
                    Scalar::Json => continue,
                    Scalar::Decimal { .. } => continue,
                    Scalar::Enum(_) => continue,
                        };
                        out.push_str(&format!("\n    pub fn {}() -> ::rustorm::relations::InverseOne<Record, super::{}::Record, {}> {{\n        ::rustorm::relations::InverseOne::new({:?}, {:?}, {:?}, {:?}, &[", rust_ident(&snake(&relation.name)), rust_ident(&snake(&child.name)), key_type, m.table, parent_key.column, child.table, child_key.column));
                        for f in &child.fields {
                            out.push_str(&format!("{:?}, ", f.column));
                        }
                        out.push_str("])");
                        if child_key.nullable {
                            out.push_str(".with_nullable_fk()");
                        }
                        generate_unique_child_fields(&mut out, child);
                        out.push_str("\n    }\n");
                    }
                }
            }
        }
        for relation in m.relations.iter().filter(|r| r.list) {
            let child = schema
                .models
                .iter()
                .find(|model| model.name == relation.target);
            if let Some(child) = child {
                if inverse_relation(m, relation, child).is_some_and(|r| r.list) {
                    if let (Some(parent_key), Some(child_key)) = (
                        m.fields.iter().find(|f| f.id),
                        child.fields.iter().find(|f| f.id),
                    ) {
                        let key_type = match parent_key.ty {
                            Scalar::Int => "i32",
                            Scalar::BigInt => "i64",
                            Scalar::Boolean => "bool",
                            Scalar::String => "String",
                            Scalar::Float => continue,
                    Scalar::DateTime => continue,
                    Scalar::Bytes => continue,
                    Scalar::Json => continue,
                    Scalar::Decimal { .. } => continue,
                    Scalar::Enum(_) => continue,
                        };
                        let child_type = match child_key.ty {
                            Scalar::Int => "i32",
                            Scalar::BigInt => "i64",
                            Scalar::Boolean => "bool",
                            Scalar::String => "String",
                            Scalar::Float => continue,
                    Scalar::DateTime => continue,
                    Scalar::Bytes => continue,
                    Scalar::Json => continue,
                    Scalar::Decimal { .. } => continue,
                    Scalar::Enum(_) => continue,
                        };
                        let (local, remote) = if m.name < child.name {
                            ("a", "b")
                        } else {
                            ("b", "a")
                        };
                        out.push_str(&format!("\n    pub fn {}() -> ::rustorm::relations::Through<Record, super::{}::Record, {}, {}> {{\n        ::rustorm::relations::Through::new({:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, &[", rust_ident(&snake(&relation.name)), rust_ident(&snake(&child.name)), key_type, child_type, m.table, parent_key.column, child.table, child_key.column, implicit_join_name(&m.name, &child.name), local, remote));
                        for f in &child.fields {
                            out.push_str(&format!("{:?}, ", f.column));
                        }
                        out.push_str("])\n    }\n");
                    }
                    continue;
                }
            }
            let info = child.and_then(|child| {
                inverse_relation(m, relation, child)
                    .and_then(|r| r.foreign_key.as_ref().map(|fk| (child, fk)))
            });
            if let Some((child, fk)) = info {
                if let (Some(parent_key), Some(child_key)) = (
                    m.fields.iter().find(|f| f.name == fk.references),
                    child.fields.iter().find(|f| f.name == fk.field),
                ) {
                    let key_type = match parent_key.ty {
                        Scalar::Int => "i32",
                        Scalar::BigInt => "i64",
                        Scalar::Boolean => "bool",
                        Scalar::String => "String",
                        Scalar::Float => continue,
                    Scalar::DateTime => continue,
                    Scalar::Bytes => continue,
                    Scalar::Json => continue,
                    Scalar::Decimal { .. } => continue,
                    Scalar::Enum(_) => continue,
                    };
                    out.push_str(&format!("\n    pub fn {}() -> ::rustorm::relations::Many<Record, super::{}::Record, {}> {{\n        ::rustorm::relations::Many::new({:?}, {:?}, {:?}, {:?}, &[", rust_ident(&snake(&relation.name)), rust_ident(&snake(&child.name)), key_type, m.table, parent_key.column, child.table, child_key.column));
                    for field in &child.fields {
                        out.push_str(&format!("{:?}, ", field.column));
                    }
                    out.push_str("])");
                    if child_key.nullable {
                        out.push_str(".with_nullable_fk()");
                    }
                    generate_unique_child_fields(&mut out, child);
                    out.push_str("\n    }\n");
                }
            }
        }
        // P1: ModelDelegate for builder API inside each model module
        out.push_str(&format!("    pub struct ModelDelegate;\n    impl ModelDelegate {{\n        pub fn find_many(&self) -> ::rustorm::client_builder::FindManyBuilder {{ ::rustorm::client_builder::FindManyBuilder::new(\"{}\", columns()) }}\n        pub fn find_unique(&self, filter: ::rustorm::query::Filter) -> ::rustorm::client_builder::FindManyBuilder {{ ::rustorm::client_builder::FindManyBuilder::new(\"{}\", columns()).filter(filter) }}\n    }}\n", m.table, m.table));
        out.push_str("}\n\n");
    }
    // P2: views (read-only)
    for v in &schema.views {
        out.push_str(&format!("pub mod {} {{\n", rust_ident(&snake(&v.name))));
        out.push_str("    #[derive(Debug, Clone, PartialEq)]\n    pub struct Record {\n");
        for f in &v.fields {
            out.push_str(&format!("        pub {}: {},\n", rust_ident(&snake(&f.name)), ty(f)));
        }
        out.push_str("    }\n\n    impl ::rustorm::row::FromRow for Record {\n        fn from_row(row: &::rustorm::row::Row) -> Result<Self, ::rustorm::row::DecodeError> {\n            Ok(Self {\n");
        for f in &v.fields {
            out.push_str(&format!("                {}: row.get({:?})?,\n", rust_ident(&snake(&f.name)), f.column));
        }
        out.push_str("            })\n        }\n    }\n");
        out.push_str(&format!("    pub fn columns() -> &'static [&'static str] {{ &[{}] }}\n", v.fields.iter().map(|f| format!("{:?}", f.column)).collect::<Vec<_>>().join(", ")));
        out.push_str(&format!("    pub fn find_many() -> ::rustorm::query::Select {{ ::rustorm::query::Select::new({:?}, columns()) }}\n", v.table));
        out.push_str("}\n\n");
    }
    // P1: RustormClient top-level (P0 §6.1 + P1 pooling)
    out.push_str("/// Generated typed client — mirrors Prisma's RustormClient.\n");
    out.push_str("pub struct RustormClient {\n    #[cfg(feature = \"postgres\")]\n    pub pool: Option<::rustorm::postgres::Pool>,\n}\n");
    out.push_str("impl RustormClient {\n    pub fn new(url: &str) -> Self { Self { #[cfg(feature = \"postgres\")] pool: Some(::rustorm::postgres::Pool::new(url.to_owned(), 5)) } }\n");
    out.push_str("    pub fn from_env() -> Result<Self, String> { let url = std::env::var(\"DATABASE_URL\").map_err(|_| \"DATABASE_URL not set\".to_string())?; Ok(Self::new(&url)) }\n");
    out.push_str("    pub fn builder() -> Self { Self::new(\"\") }\n");
    for m in &schema.models {
        out.push_str(&format!("    /// accessor for model {}\n    pub fn {}(&self) -> {}Delegate {{ {}Delegate }}\n", m.name, rust_ident(&snake(&m.name)), m.name, m.name));
    }
    out.push_str("}\n\n");
    // per-model ModelDelegate inside each model module (builder API §6.2)
    for m in &schema.models {
        let mod_name = rust_ident(&snake(&m.name));
        out.push_str(&format!("// per-model delegation handled via top-level delegates below\n"));
    }
    // Top-level delegates for each model (P1)
    for m in &schema.models {
        let mod_name = rust_ident(&snake(&m.name));
        out.push_str(&format!("pub struct {}Delegate;\nimpl {}Delegate {{\n    pub fn find_many(&self) -> ::rustorm::client_builder::FindManyBuilder {{ ::rustorm::client_builder::FindManyBuilder::new(\"{}\", {}::columns()) }}\n    pub fn find_unique(&self, filter: ::rustorm::query::Filter) -> ::rustorm::client_builder::FindManyBuilder {{ ::rustorm::client_builder::FindManyBuilder::new(\"{}\", {}::columns()).filter(filter) }}\n    pub fn find_first(&self) -> ::rustorm::client_builder::FindManyBuilder {{ ::rustorm::client_builder::FindManyBuilder::new(\"{}\", {}::columns()) }}\n}}\n", m.name, m.name, m.table, mod_name, m.table, mod_name, m.table, mod_name));
    }
    // P2: composite types
    for c in &schema.composites {

        out.push_str(&format!("pub mod {} {{ // {}\n", rust_ident(&snake(&c.name)), c.name));
        out.push_str("    #[derive(Debug, Clone, PartialEq)]\n    pub struct Composite { // Composite\n");
        for f in &c.fields {
            out.push_str(&format!("        pub {}: {},\n", rust_ident(&snake(&f.name)), ty(f)));
        }
        out.push_str("    }\n}\n\n");
    }
    out
}
