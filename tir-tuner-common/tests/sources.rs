//! Every number on the medical path traces to `docs/sources.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::visit::{self, Visit};

struct Const {
    line: usize,
    tags: Vec<String>,
    values: Vec<f64>,
}

/// Numbers outside test code: inside consts, and loose ones.
#[derive(Default)]
struct Scan {
    consts: Vec<Const>,
    loose: Vec<(usize, f64)>,
    /// Literals of the const being visited, if any.
    current: Option<Vec<f64>>,
}

fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>()
                .is_ok_and(|m| m.path().is_ident("test"))
    })
}

fn tag_re() -> regex::Regex {
    regex::Regex::new(r"src: ([a-z0-9-]+)").unwrap()
}

fn tags(attrs: &[syn::Attribute]) -> Vec<String> {
    let tag = tag_re();
    attrs
        .iter()
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) if nv.path.is_ident("doc") => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|doc| {
            tag.captures_iter(&doc)
                .map(|c| c[1].to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

impl Scan {
    fn number(&mut self, line: usize, v: f64) {
        match &mut self.current {
            Some(values) => values.push(v),
            None => self.loose.push((line, v)),
        }
    }

    fn constant(&mut self, attrs: &[syn::Attribute], line: usize, expr: &syn::Expr) {
        self.current = Some(Vec::new());
        self.visit_expr(expr);
        let values = self.current.take().unwrap();
        self.consts.push(Const {
            line,
            tags: tags(attrs),
            values,
        });
    }
}

impl<'a> Visit<'a> for Scan {
    fn visit_item(&mut self, i: &'a syn::Item) {
        let attrs: &[syn::Attribute] = match i {
            syn::Item::Mod(m) => &m.attrs,
            syn::Item::Fn(f) => &f.attrs,
            syn::Item::Impl(m) => &m.attrs,
            syn::Item::Const(c) => &c.attrs,
            _ => &[],
        };
        if !is_test(attrs) {
            visit::visit_item(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'a syn::ImplItemFn) {
        if !is_test(&f.attrs) {
            visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_item_const(&mut self, c: &'a syn::ItemConst) {
        self.constant(&c.attrs, c.const_token.span.start().line, &c.expr);
    }

    fn visit_impl_item_const(&mut self, c: &'a syn::ImplItemConst) {
        self.constant(&c.attrs, c.const_token.span.start().line, &c.expr);
    }

    /// Array lengths count elements, they aren't quantities.
    fn visit_type_array(&mut self, t: &'a syn::TypeArray) {
        self.visit_type(&t.elem);
    }

    /// syn doesn't parse macro arguments, so walk their tokens.
    fn visit_macro(&mut self, m: &'a syn::Macro) {
        fn walk(s: &mut Scan, tokens: proc_macro2::TokenStream) {
            for t in tokens {
                match t {
                    proc_macro2::TokenTree::Group(g) => walk(s, g.stream()),
                    proc_macro2::TokenTree::Literal(l) => match syn::Lit::new(l) {
                        syn::Lit::Int(i) => s.visit_lit_int(&i),
                        syn::Lit::Float(f) => s.visit_lit_float(&f),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        walk(self, m.tokens.clone());
    }

    fn visit_lit_int(&mut self, l: &'a syn::LitInt) {
        self.number(
            l.span().start().line,
            l.base10_parse::<u64>().unwrap() as f64,
        );
    }

    fn visit_lit_float(&mut self, l: &'a syn::LitFloat) {
        self.number(l.span().start().line, l.base10_parse::<f64>().unwrap());
    }
}

fn read(root: &Path, file: &str) -> String {
    std::fs::read_to_string(root.join(file)).unwrap()
}

fn strings(v: &toml::Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn numbers(v: &toml::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v.as_float()
                .or_else(|| v.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

#[test]
fn medical_numbers_trace_to_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let toml: toml::Table = read(&root, "docs/sources.toml").parse().unwrap();
    let sources = toml["sources"].as_table().unwrap();
    let mut claims: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut errors = Vec::new();
    for c in toml["claim"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap().to_string();
        let source = c["source"].as_str().unwrap();
        if !sources.contains_key(source) {
            errors.push(format!("claim {id}: unknown source {source}"));
        }
        let values = c.get("values").map_or(vec![], numbers);
        if claims.insert(id.clone(), values).is_some() {
            errors.push(format!("claim {id} appears twice"));
        }
    }
    let allowed = numbers(&toml["lint"]["allowed"]);
    let files = strings(&toml["lint"]["files"]);

    let mutants: toml::Table = read(&root, ".cargo/mutants.toml").parse().unwrap();
    if strings(&mutants["examine_globs"]) != files {
        errors.push(".cargo/mutants.toml examine_globs differs from lint.files".into());
    }

    // Tags anywhere, comments included; an unknown id fails.
    let tag = tag_re();
    let mut used = BTreeSet::new();
    let mut const_values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for f in &files {
        let src = read(&root, f);
        for (n, line) in src.lines().enumerate() {
            for c in tag.captures_iter(line) {
                if !claims.contains_key(&c[1]) {
                    errors.push(format!(
                        "{f}:{}: src: {} is not in docs/sources.toml",
                        n + 1,
                        &c[1]
                    ));
                }
                used.insert(c[1].to_string());
            }
        }
        let mut s = Scan::default();
        s.visit_file(&syn::parse_file(&src).unwrap());
        for c in &s.consts {
            if c.values.iter().any(|v| !allowed.contains(v)) && c.tags.is_empty() {
                errors.push(format!(
                    "{f}:{}: const with a number but no src: tag",
                    c.line
                ));
            }
            for t in &c.tags {
                const_values.entry(t.clone()).or_default().extend(&c.values);
            }
        }
        for (line, v) in s.loose.iter().filter(|(_, v)| !allowed.contains(v)) {
            errors.push(format!("{f}:{line}: literal {v} outside a tagged const"));
        }
    }
    for (id, values) in &claims {
        if !used.contains(id) {
            errors.push(format!("claim {id} is not used in code"));
        }
        let in_code = const_values.get(id).cloned().unwrap_or_default();
        for v in values.iter().filter(|v| !in_code.contains(v)) {
            errors.push(format!(
                "claim {id}: value {v} is not in a const tagged with it"
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}
