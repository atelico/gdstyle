//! Tokens and parsed members of recently seen sources, reused while the
//! formatter runs.
//!
//! The formatter's passes lex and parse the same text several times: the
//! lint and the fixer both read a pass's output, the reorder guard and the
//! reorder parse the same source, and a pass that changes nothing hands the
//! next one the same text. Lexing and parsing are most of a pass's cost, so
//! [`with_source_cache`] keeps the results for the few texts alive at once.
//! Outside it every call lexes and parses afresh, so library callers of
//! `lint_source` and friends never see cached state.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use crate::ast::ClassMember;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::token::Token;

/// How many recent sources the cache keeps. A formatter pass rarely has
/// more than a few versions of the text alive at once.
const CACHED_SOURCES: usize = 4;

/// One source's tokens, and its members once something asked for them.
struct CachedSource {
    text: String,
    tokens: Rc<Vec<Token>>,
    members: OnceCell<Rc<Vec<ClassMember>>>,
}

thread_local! {
    /// The cached sources, newest last, or `None` outside
    /// [`with_source_cache`].
    static CACHE: RefCell<Option<Vec<Rc<CachedSource>>>> = const { RefCell::new(None) };
}

/// Run `body` with [`tokens_of`] and [`members_of`] reusing their results for
/// a source they have already seen during `body`. The cache lives only for
/// `body` and only on this thread.
pub(crate) fn with_source_cache<T>(body: impl FnOnce() -> T) -> T {
    /// Drops the cache when `body` returns or unwinds, unless an outer call
    /// owns it.
    struct ClearOnDrop {
        owns_cache: bool,
    }
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            if self.owns_cache {
                CACHE.with(|cache| *cache.borrow_mut() = None);
            }
        }
    }
    let owns_cache = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let is_unset = cache.is_none();
        if is_unset {
            *cache = Some(Vec::with_capacity(CACHED_SOURCES));
        }
        is_unset
    });
    let _clear = ClearOnDrop { owns_cache };
    body()
}

/// The tokens of `source`.
pub(crate) fn tokens_of(source: &str) -> Rc<Vec<Token>> {
    match cached_source(source) {
        Some(cached) => Rc::clone(&cached.tokens),
        None => Rc::new(Lexer::new(source).tokenize()),
    }
}

/// The class members `source` parses to. `tokens` must be `source`'s
/// tokens; they are parsed unless the cache already holds the members.
pub(crate) fn members_of(source: &str, tokens: &[Token]) -> Rc<Vec<ClassMember>> {
    let parse = || Rc::new(Parser::new(tokens).parse());
    match cached_source(source) {
        Some(cached) => Rc::clone(cached.members.get_or_init(parse)),
        None => parse(),
    }
}

/// The cache entry for `source`, lexing it on a miss. `None` when no
/// [`with_source_cache`] call is running.
fn cached_source(source: &str) -> Option<Rc<CachedSource>> {
    let found = CACHE.with(|cache| {
        cache.borrow().as_ref().map(|entries| {
            entries
                .iter()
                .find(|cached| cached.text == source)
                .map(Rc::clone)
        })
    })?;
    if let Some(cached) = found {
        return Some(cached);
    }
    let cached = Rc::new(CachedSource {
        text: source.to_string(),
        tokens: Rc::new(Lexer::new(source).tokenize()),
        members: OnceCell::new(),
    });
    CACHE.with(|cache| {
        if let Some(entries) = cache.borrow_mut().as_mut() {
            if entries.len() == CACHED_SOURCES {
                entries.remove(0);
            }
            entries.push(Rc::clone(&cached));
        }
    });
    Some(cached)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_results_only_inside_the_cache_scope() {
        let source = "var a = 1\n";
        assert!(!Rc::ptr_eq(&tokens_of(source), &tokens_of(source)));
        with_source_cache(|| {
            assert!(Rc::ptr_eq(&tokens_of(source), &tokens_of(source)));
            let tokens = tokens_of(source);
            assert!(Rc::ptr_eq(
                &members_of(source, &tokens),
                &members_of(source, &tokens)
            ));
            assert_eq!(members_of(source, &tokens).len(), 1);
        });
        let tokens = tokens_of(source);
        assert!(!Rc::ptr_eq(
            &members_of(source, &tokens),
            &members_of(source, &tokens)
        ));
    }

    #[test]
    fn keeps_the_newest_sources() {
        with_source_cache(|| {
            let first = tokens_of("var a = 1\n");
            for index in 0..CACHED_SOURCES {
                tokens_of(&format!("var b{index} = 1\n"));
            }
            assert!(!Rc::ptr_eq(&first, &tokens_of("var a = 1\n")));
        });
    }
}
