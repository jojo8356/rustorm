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
pub struct Schema {
    pub models: Vec<Model>,
    pub generator_output: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: String,
    pub table: String,
    pub fields: Vec<Field>,
    /// Virtual relation fields; these are not physical database columns.
    pub relations: Vec<Relation>,
    /// Explicit single-column indexes.
    pub indexes: Vec<Index>,
    /// Logical field names of an @@id; empty when a scalar field has @id.
    pub primary_key: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub field: String,
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
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    Int,
    BigInt,
    Float,
    Boolean,
    String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultValue {
    Autoincrement,
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
    fn lines(&mut self) {
        while self.peek().kind == Kind::Newline {
            self.next();
        }
    }
    fn end_line(&mut self) -> Result<(), Diagnostic> {
        match self.peek().kind {
            Kind::Newline => {
                self.lines();
                Ok(())
            }
            Kind::Symbol('}') => Ok(()),
            _ => Err(err(self.peek(), "expected end of line")),
        }
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
                    let (value, _) = self.ident()?;
                    self.symbol(']')?;
                    if name == "fields" {
                        fields = Some(value);
                    } else {
                        references = Some(value);
                    }
                }
                "onDelete" | "onUpdate" => {
                    let (value, tok) = self.ident()?;
                    let action = match value.as_str() {
                        "Cascade" => ReferentialAction::Cascade,
                        "Restrict" => ReferentialAction::Restrict,
                        "NoAction" => ReferentialAction::NoAction,
                        "SetNull" => ReferentialAction::SetNull,
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
                        let (field, field_tok) = self.ident()?;
                        self.symbol(']')?;
                        self.symbol(')')?;
                        model.indexes.push(Index {
                            field,
                            line: field_tok.line,
                            column: field_tok.col,
                        });
                    }
                    _ => return Err(err(&tok, format!("unsupported model attribute '@@{attr}'"))),
                }
                self.end_line()?;
                continue;
            }
            let (field_name, tok) = self.ident()?;
            let (type_name, type_tok) = self.ident()?;
            if !["Int", "BigInt", "Float", "Boolean", "String"].contains(&type_name.as_str()) {
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
                if self.is('@') {
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
            let ty = match type_name.as_str() {
                "Int" => Scalar::Int,
                "BigInt" => Scalar::BigInt,
                "Float" => Scalar::Float,
                "Boolean" => Scalar::Boolean,
                "String" => Scalar::String,
                _ => {
                    return Err(err(
                        &type_tok,
                        format!("unsupported field type '{type_name}'"),
                    ))
                }
            };
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
            };
            let mut attrs = HashSet::new();
            while self.is('@') {
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
                            _ => return Err(err(&val, "unsupported default value")),
                        });
                        self.symbol(')')?;
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
                let valid = match (field.ty, default) {
                    (Scalar::Int | Scalar::BigInt, DefaultValue::Autoincrement) => field.id,
                    (Scalar::String, DefaultValue::String(_))
                    | (Scalar::Boolean, DefaultValue::Boolean(_)) => true,
                    (Scalar::Int, DefaultValue::Number(n)) => n.parse::<i32>().is_ok(),
                    (Scalar::BigInt, DefaultValue::Number(n)) => n.parse::<i64>().is_ok(),
                    (Scalar::Float, DefaultValue::Number(n)) => {
                        n.parse::<f64>().is_ok_and(|v| v.is_finite())
                    }
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
    if models.is_empty() {
        return Err(err(p.peek(), "schema must contain a model"));
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
    let schema = Schema {
        models,
        generator_output: output,
    };
    validate_indexes(&schema)?;
    validate_relations(&schema)?;
    Ok(schema)
}
/// Validate single-column scalar indexes, including on externally constructed schemas.
pub fn validate_indexes(schema: &Schema) -> Result<(), Diagnostic> {
    for model in &schema.models {
        let mut seen = HashSet::new();
        for index in &model.indexes {
            let err = || Diagnostic {
                line: index.line,
                column: index.column,
                message: format!(
                    "invalid or duplicate index field '{}' in '{}'",
                    index.field, model.name
                ),
            };
            if !seen.insert(&index.field)
                || model
                    .fields
                    .iter()
                    .filter(|field| field.name == index.field)
                    .count()
                    != 1
            {
                return Err(err());
            }
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
                    if is_self || relation.relation_name.is_some() {
                        return Err(fail("named or self implicit many-to-many is not supported"));
                    }
                    if other.foreign_key.is_some() || other.nullable {
                        return Err(fail("invalid implicit many-to-many inverse"));
                    }
                    let id = model
                        .fields
                        .iter()
                        .find(|f| f.id)
                        .ok_or_else(|| fail("implicit many-to-many requires a primary key"))?;
                    if id.nullable || id.ty == Scalar::Float {
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
            let local: Vec<_> = model.fields.iter().filter(|f| f.name == fk.field).collect();
            let remote: Vec<_> = target
                .fields
                .iter()
                .filter(|f| f.name == fk.references)
                .collect();
            if local.len() != 1 || remote.len() != 1 {
                return Err(fail("foreign key fields must exist and be unambiguous"));
            }
            if local[0].ty == Scalar::Float {
                return Err(fail("Float relation keys are not supported"));
            }
            if local[0].ty != remote[0].ty
                || !(remote[0].id || remote[0].unique)
                || remote[0].nullable
            {
                return Err(fail(
                    "foreign key must reference a compatible non-null unique field",
                ));
            }
            if other.list && (local[0].id || local[0].unique) {
                return Err(fail("one-to-many foreign key cannot be unique"));
            }
            if !other.list && !(local[0].id || local[0].unique) {
                return Err(fail("one-to-one foreign key must be unique"));
            }
            if relation.nullable != local[0].nullable {
                return Err(fail("relation and foreign key nullability must match"));
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
fn ty(f: &Field) -> String {
    let base = match f.ty {
        Scalar::Int => "i32",
        Scalar::BigInt => "i64",
        Scalar::Float => "f64",
        Scalar::Boolean => "bool",
        Scalar::String => "String",
    };
    if f.nullable {
        format!("Option<{base}>")
    } else {
        base.to_owned()
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
pub fn generate(schema: &Schema) -> String {
    let mut out = String::from("// @generated by rustorm; do not edit.\n\n");
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
                        };
                        let child_type = match child_key.ty {
                            Scalar::Int => "i32",
                            Scalar::BigInt => "i64",
                            Scalar::Boolean => "bool",
                            Scalar::String => "String",
                            Scalar::Float => continue,
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
        out.push_str("}\n\n");
    }
    out
}
