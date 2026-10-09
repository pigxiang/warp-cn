use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{
    Expr, ExprLit, ExprMacro, ExprMethodCall, File, ItemImpl, ItemMod, Lit, LitStr, Meta, StmtMacro,
};

use super::allowlist::Allowlist;
use super::report::{Severity, Violation};
use super::sites::SiteTable;

pub fn scan_file(
    rel_path: &Path,
    _src: &str,
    file: &File,
    sites: &SiteTable,
    allowlist: &Allowlist,
) -> Vec<Violation> {
    let mut v = Visitor {
        rel_path: rel_path.to_path_buf(),
        sites,
        allowlist,
        violations: Vec::new(),
        suppressed_depth: usize::from(is_test_filename(rel_path)),
        in_fmt_impl_depth: 0,
    };
    v.visit_file(file);
    v.violations
}

fn is_test_filename(p: &Path) -> bool {
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.ends_with("_test") || stem.ends_with("_tests") || stem == "tests"
}

struct Visitor<'a> {
    rel_path: PathBuf,
    sites: &'a SiteTable,
    allowlist: &'a Allowlist,
    violations: Vec<Violation>,
    suppressed_depth: usize,
    in_fmt_impl_depth: usize,
}

impl<'a> Visitor<'a> {
    fn suppressed(&self) -> bool {
        self.suppressed_depth > 0 || self.in_fmt_impl_depth > 0
    }

    fn record(
        &mut self,
        span: Span,
        callsite: String,
        literal: String,
        msg: String,
        sev: Severity,
    ) {
        let start = span.start();
        self.violations.push(Violation {
            file: self.rel_path.clone(),
            line: start.line,
            column: start.column + 1,
            callsite,
            literal,
            severity: sev,
            message: msg,
        });
    }

    fn check_ui_literal(&mut self, callsite: &str, lit: &LitStr) {
        if self.suppressed() {
            return;
        }
        let value = lit.value();
        if !is_user_visible_string(&value) {
            return;
        }
        if self.allowlist.allows(&self.rel_path, callsite, &value) {
            return;
        }
        self.record(
            lit.span(),
            callsite.to_string(),
            value,
            "untranslated UI literal — wrap in t!()".to_string(),
            Severity::Soft,
        );
    }

    fn check_translate_macro(&mut self, mac_path: &syn::Path, tokens: &TokenStream) {
        // Scan every macro argument: `tr!(Locale::En, key)` puts the key in
        // arg[1], and named interpolation args (`t!("k", n: count)`) may also
        // carry server data.
        let macro_name = mac_path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| "t".to_string());
        for arg in macro_arg_tokens(tokens) {
            if arg.is_empty() || arg_is_plain_string_literal(&arg) {
                continue;
            }
            if let Some(reason) = detect_remote_string_flow(&arg) {
                let callsite = format!("{macro_name}!");
                let literal = reduce_to_str(&arg);
                if self.allowlist.allows(&self.rel_path, &callsite, &literal) {
                    continue;
                }
                let span = arg
                    .first()
                    .map(TokenTree::span)
                    .unwrap_or_else(Span::call_site);
                self.record(
                    span,
                    callsite,
                    literal,
                    format!(
                        "RemoteString cannot be passed to t!/tr! ({reason}); render server text directly"
                    ),
                    Severity::Hard,
                );
            }
        }
    }
}

impl<'ast, 'a> Visit<'ast> for Visitor<'a> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        let bump = m.attrs.iter().any(attr_is_cfg_test) || m.ident == "tests";
        if bump {
            self.suppressed_depth += 1;
        }
        visit::visit_item_mod(self, m);
        if bump {
            self.suppressed_depth -= 1;
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        let bump = impl_is_fmt_debug_or_display(i);
        if bump {
            self.in_fmt_impl_depth += 1;
        }
        visit::visit_item_impl(self, i);
        if bump {
            self.in_fmt_impl_depth -= 1;
        }
    }

    fn visit_expr_macro(&mut self, m: &'ast ExprMacro) {
        if mac_is_exempt(&m.mac.path) {
            return;
        }
        if mac_is_translate(&m.mac.path) {
            self.check_translate_macro(&m.mac.path, &m.mac.tokens);
        }
        visit::visit_expr_macro(self, m);
    }

    fn visit_stmt_macro(&mut self, m: &'ast StmtMacro) {
        if mac_is_exempt(&m.mac.path) {
            return;
        }
        if mac_is_translate(&m.mac.path) {
            self.check_translate_macro(&m.mac.path, &m.mac.tokens);
        }
        visit::visit_stmt_macro(self, m);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = c.func.as_ref() {
            let path_repr = path_to_string(&p.path);
            if self.sites.is_call_site(&path_repr) {
                if let Some(lit) = first_str_literal(&c.args) {
                    self.check_ui_literal(&path_repr, lit);
                }
            }
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast ExprMethodCall) {
        let method = m.method.to_string();
        if self.sites.is_method_site(&method) {
            if let Some(lit) = first_str_literal(&m.args) {
                self.check_ui_literal(&format!(".{method}"), lit);
            }
        }
        visit::visit_expr_method_call(self, m);
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        if c.ident.to_string().starts_with('_') {
            return;
        }
        visit::visit_item_const(self, c);
    }
}

fn first_str_literal(args: &syn::punctuated::Punctuated<Expr, syn::Token![,]>) -> Option<&LitStr> {
    args.iter().next().and_then(str_lit_expr)
}

fn str_lit_expr(expr: &Expr) -> Option<&LitStr> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s),
        Expr::Reference(r) => str_lit_expr(&r.expr),
        Expr::Paren(p) => str_lit_expr(&p.expr),
        Expr::MethodCall(m)
            if m.args.is_empty()
                && matches!(
                    m.method.to_string().as_str(),
                    "into" | "to_owned" | "to_string"
                ) =>
        {
            str_lit_expr(&m.receiver)
        }
        Expr::Call(c) if call_is_string_from(c.func.as_ref()) => {
            c.args.iter().next().and_then(str_lit_expr)
        }
        _ => None,
    }
}

fn call_is_string_from(func: &Expr) -> bool {
    matches!(func, Expr::Path(p) if path_to_string(&p.path).ends_with("String::from"))
}

fn path_to_string(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn attr_is_cfg_test(attr: &syn::Attribute) -> bool {
    let Meta::List(list) = &attr.meta else {
        return false;
    };
    if !list.path.is_ident("cfg") {
        return false;
    }
    cfg_tokens_have_positive_test(&list.tokens)
}

/// Recursive cfg parser that returns true only when the predicate evaluates
/// to `test` in a *positive* position, i.e. matches `test`, `all(..., test)`,
/// or `any(..., test)`. `not(test)` does NOT count as suppressing — the body
/// of a `cfg(not(test))` module is production code.
fn cfg_tokens_have_positive_test(tokens: &TokenStream) -> bool {
    let mut iter = tokens.clone().into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                if name == "test" {
                    return true;
                }
                if name == "not" {
                    // Skip the negated group entirely.
                    if matches!(iter.peek(), Some(TokenTree::Group(_))) {
                        iter.next();
                    }
                } else if matches!(name.as_str(), "all" | "any")
                    && let Some(TokenTree::Group(group)) = iter.peek()
                {
                    let group = group.clone();
                    iter.next();
                    if cfg_tokens_have_positive_test(&group.stream()) {
                        return true;
                    }
                }
            }
            TokenTree::Group(group) => {
                if cfg_tokens_have_positive_test(&group.stream()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn impl_is_fmt_debug_or_display(i: &ItemImpl) -> bool {
    let Some((_, trait_path, _)) = &i.trait_ else {
        return false;
    };
    let Some(last) = trait_path.segments.last() else {
        return false;
    };
    let name = last.ident.to_string();
    if name != "Debug" && name != "Display" {
        return false;
    }
    if trait_path.segments.len() >= 2 {
        let prev = &trait_path.segments[trait_path.segments.len() - 2];
        return prev.ident == "fmt";
    }
    true
}

fn mac_is_exempt(path: &syn::Path) -> bool {
    let last = match path.segments.last() {
        Some(s) => s.ident.to_string(),
        None => return false,
    };
    matches!(
        last.as_str(),
        "println"
            | "eprintln"
            | "print"
            | "eprint"
            | "panic"
            | "todo"
            | "unimplemented"
            | "unreachable"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "format"
            | "write"
            | "writeln"
            | "info"
            | "warn"
            | "error"
            | "debug"
            | "trace"
            | "log"
            | "dbg"
    )
}

fn mac_is_translate(path: &syn::Path) -> bool {
    let Some(last) = path.segments.last() else {
        return false;
    };
    let name = last.ident.to_string();
    name == "t" || name == "tr"
}

fn macro_arg_tokens(tokens: &TokenStream) -> Vec<Vec<TokenTree>> {
    let mut args = Vec::new();
    let mut current = Vec::new();
    for tt in tokens.clone() {
        if let TokenTree::Punct(ref p) = tt
            && p.as_char() == ','
        {
            args.push(std::mem::take(&mut current));
            continue;
        }
        current.push(tt);
    }
    args.push(current);
    args
}

fn arg_is_plain_string_literal(tokens: &[TokenTree]) -> bool {
    let stream: TokenStream = tokens.iter().cloned().collect();
    syn::parse2::<LitStr>(stream).is_ok()
}

/// Detect a non-literal `t!`/`tr!` argument carrying possible server-provided
/// text. Per spec D7 these matches stay hard regardless of `--mode`; legit
/// local-`String` cases must be added to `lint_allowlist.toml`.
fn detect_remote_string_flow(tokens: &[TokenTree]) -> Option<&'static str> {
    let s = reduce_to_str(tokens);
    if s.contains("RemoteString") {
        return Some("argument mentions RemoteString");
    }
    if s.contains(".as_str()") || s.contains(".as_ref()") || s.contains(".into_string()") {
        return Some("argument unwraps a non-literal string source");
    }
    if has_known_remote_field_access(tokens) {
        return Some("argument reads a field commonly backed by RemoteString");
    }
    None
}

/// Token-stream walk: find `.<ident>` where `<ident>` ∈ KNOWN_REMOTE_FIELDS.
fn has_known_remote_field_access(tokens: &[TokenTree]) -> bool {
    let mut saw_dot = false;
    for token in tokens {
        match token {
            TokenTree::Punct(p) if p.as_char() == '.' => saw_dot = true,
            TokenTree::Ident(ident) if saw_dot => {
                if is_known_remote_field_name(&ident.to_string()) {
                    return true;
                }
                saw_dot = false;
            }
            TokenTree::Group(group) => {
                let nested: Vec<_> = group.stream().into_iter().collect();
                if has_known_remote_field_access(&nested) {
                    return true;
                }
                saw_dot = false;
            }
            _ => saw_dot = false,
        }
    }
    false
}

fn is_known_remote_field_name(name: &str) -> bool {
    matches!(name, "message" | "title" | "body" | "description")
}

fn reduce_to_str(tokens: &[TokenTree]) -> String {
    let mut s = String::new();
    for t in tokens {
        s.push_str(&ToString::to_string(t));
    }
    s
}

fn is_user_visible_string(s: &str) -> bool {
    let s = s.trim();
    s.chars().count() >= 2 && s.chars().any(char::is_alphabetic)
}
