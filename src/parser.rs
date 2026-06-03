use crate::ast::*;
use crate::token::*;

/// A lightweight parser that extracts class structure from GDScript tokens.
///
/// This is not a full compiler parser. It understands declarations and class
/// structure well enough for linting, but does not parse expression trees
/// or validate control flow.
///
/// The parser borrows its tokens, so callers can keep the same `Vec<Token>`
/// alive for both `Parser::new` and downstream rule consumers without
/// cloning the (sometimes ~50k-token) buffer.
pub struct Parser<'t> {
    tokens: &'t [Token],
    position: usize,
}

/// Convert pending unknown annotations into standalone class-level
/// annotation members. Called before processing `class_name`/`extends`
/// (neither accepts a leading annotation), so the annotation doesn't
/// dangle and silently attach to the first var/func found later in the
/// file. See the `@abstract` duplication bug for what happens when it
/// does.
fn flush_pending_class_annotations(
    members: &mut Vec<ClassMember>,
    pending: &mut Vec<AnnotationInfo>,
) {
    for a in std::mem::take(pending) {
        members.push(ClassMember::ClassAnnotation {
            name: a.name,
            span: a.span,
        });
    }
}

impl<'t> Parser<'t> {
    pub fn new(tokens: &'t [Token]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Parse a GDScript file into a list of class members.
    pub fn parse(&mut self) -> Vec<ClassMember> {
        let mut members = Vec::new();
        let mut pending_annotations: Vec<AnnotationInfo> = Vec::new();

        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }

            match self.current_kind() {
                TokenKind::Annotation(_) => {
                    let annotation = self.parse_annotation();
                    match annotation.name.as_str() {
                        "tool" => members.push(ClassMember::ToolAnnotation {
                            span: annotation.span,
                        }),
                        "icon" => {
                            self.skip_annotation_args();
                            members.push(ClassMember::IconAnnotation {
                                span: annotation.span,
                            });
                        }
                        "static_unload" => {
                            members.push(ClassMember::StaticUnloadAnnotation {
                                span: annotation.span,
                            });
                        }
                        "warning_ignore" | "warning_ignore_start" | "warning_ignore_restore" => {
                            self.skip_annotation_args();
                        }
                        _ => {
                            self.skip_annotation_args();
                            pending_annotations.push(annotation);
                        }
                    }
                }
                TokenKind::ClassName => {
                    // Any annotation still pending here can't belong to
                    // class_name itself — class_name doesn't accept
                    // annotations as a prefix. Flush them as standalone
                    // class-level annotations so they don't quietly
                    // ride forward and attach to the next function in
                    // the file (which is what produced the @abstract
                    // duplication bug: the formatter then thought the
                    // function "owned" lines 1..N and re-emitted them).
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_class_name();
                    members.push(member);
                }
                TokenKind::Extends => {
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_extends();
                    members.push(member);
                }
                TokenKind::DocComment(_) => {
                    let member = self.parse_doc_comment();
                    members.push(member);
                }
                TokenKind::Comment(_) => {
                    let token = self.current_token().clone();
                    self.advance();
                    if let TokenKind::Comment(text) = &token.kind {
                        members.push(ClassMember::Comment {
                            text: text.clone(),
                            is_doc: false,
                            span: token.span,
                        });
                    }
                }
                TokenKind::Signal => {
                    // Same rationale as class_name/extends: signal
                    // declarations don't take a leading annotation, so
                    // anything pending here is a class-level annotation
                    // that landed before the signal.
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_signal();
                    members.push(member);
                }
                TokenKind::Enum => {
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_enum();
                    members.push(member);
                }
                TokenKind::Const => {
                    // `const` doesn't currently accept annotations
                    // either. Flush so a stray @abstract above a const
                    // doesn't ride forward to the next function.
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_const();
                    members.push(member);
                }
                TokenKind::Static => {
                    let member = self.parse_static_member(std::mem::take(&mut pending_annotations));
                    members.push(member);
                }
                TokenKind::Var => {
                    let annotations = std::mem::take(&mut pending_annotations);
                    let member = self.parse_variable(annotations);
                    members.push(member);
                }
                TokenKind::Func => {
                    let annotations = std::mem::take(&mut pending_annotations);
                    let member = self.parse_function(false, annotations);
                    members.push(member);
                }
                TokenKind::Class => {
                    // Inner class declarations don't currently accept a
                    // leading annotation (the AST has no slot for
                    // them). Flush as class-level so the annotation
                    // doesn't ride past the inner class and attach to
                    // a function further down. Note: this means
                    // `@some_annotation\nclass Inner:` is treated as if
                    // the annotation applied to the outer class; if
                    // Godot ever wires inner-class annotations through
                    // the AST, revisit this site.
                    flush_pending_class_annotations(&mut members, &mut pending_annotations);
                    let member = self.parse_inner_class();
                    members.push(member);
                }
                _ => {
                    // Skip tokens we don't understand at the top level.
                    self.advance();
                }
            }
        }

        // Anything still pending at EOF can't attach to a following
        // declaration (there isn't one). Emit as class-level so it
        // shows up in the formatter's member list and round-trips
        // through `gdstyle fmt` instead of being silently dropped.
        flush_pending_class_annotations(&mut members, &mut pending_annotations);

        members
    }

    fn parse_annotation(&mut self) -> AnnotationInfo {
        let token = self.current_token().clone();
        let name = if let TokenKind::Annotation(ref name) = token.kind {
            name.clone()
        } else {
            String::new()
        };
        self.advance();
        AnnotationInfo {
            name,
            span: token.span,
        }
    }

    fn skip_annotation_args(&mut self) {
        if !self.is_at_end() && self.current_kind() == TokenKind::LeftParen {
            let mut depth = 1;
            self.advance(); // skip (
            while !self.is_at_end() && depth > 0 {
                match self.current_kind() {
                    TokenKind::LeftParen => depth += 1,
                    TokenKind::RightParen => depth -= 1,
                    _ => {}
                }
                self.advance();
            }
        }
    }

    fn parse_class_name(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip class_name

        let (name, name_span) = if let TokenKind::Identifier(ref name) = self.current_kind() {
            let n = name.clone();
            let s = self.current_token().span;
            self.advance();
            (n, s)
        } else {
            (String::new(), Span::new(0, 0, 0, 0))
        };

        ClassMember::ClassNameDecl {
            name,
            name_span,
            span,
        }
    }

    fn parse_extends(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip extends

        let mut base = String::new();
        // Read the base class, which can include dots (e.g., "Node2D" or "some.path").
        while !self.is_at_end() {
            match self.current_kind() {
                TokenKind::Identifier(ref name) => {
                    base.push_str(name);
                    self.advance();
                    if !self.is_at_end() && self.current_kind() == TokenKind::Dot {
                        base.push('.');
                        self.advance();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        ClassMember::ExtendsDecl { base, span }
    }

    fn parse_doc_comment(&mut self) -> ClassMember {
        let token = self.current_token().clone();
        self.advance();
        let text = if let TokenKind::DocComment(ref text) = token.kind {
            text.clone()
        } else {
            String::new()
        };

        ClassMember::DocComment {
            text,
            span: token.span,
        }
    }

    fn parse_signal(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip signal

        let (name, name_span) = self.expect_identifier();
        let parameters = if !self.is_at_end() && self.current_kind() == TokenKind::LeftParen {
            self.parse_parameter_list()
        } else {
            Vec::new()
        };

        ClassMember::Signal {
            name,
            name_span,
            parameters,
            span,
        }
    }

    fn parse_enum(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip enum

        let (name, name_span) = if !self.is_at_end() {
            if let TokenKind::Identifier(ref name) = self.current_kind() {
                let n = name.clone();
                let s = self.current_token().span;
                self.advance();
                (Some(n), Some(s))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let mut enum_members = Vec::new();

        if !self.is_at_end() && self.current_kind() == TokenKind::LeftBrace {
            self.advance(); // skip {
            while !self.is_at_end() && self.current_kind() != TokenKind::RightBrace {
                self.skip_newlines();
                if self.is_at_end() || self.current_kind() == TokenKind::RightBrace {
                    break;
                }

                // Skip doc comments and comments inside enums.
                if matches!(
                    self.current_kind(),
                    TokenKind::DocComment(_) | TokenKind::Comment(_)
                ) {
                    self.advance();
                    continue;
                }

                if let TokenKind::Identifier(ref name) = self.current_kind() {
                    let member_span = self.current_token().span;
                    let member_name = name.clone();
                    self.advance();

                    // Skip optional value assignment.
                    if !self.is_at_end() && self.current_kind() == TokenKind::Assign {
                        self.advance(); // skip =
                        self.skip_expression();
                    }

                    enum_members.push(EnumMember {
                        name: member_name,
                        span: member_span,
                    });
                } else {
                    // Unknown token inside enum, skip it to avoid infinite loop.
                    self.advance();
                    continue;
                }

                // Skip comma.
                if !self.is_at_end() && self.current_kind() == TokenKind::Comma {
                    self.advance();
                }
            }
            if !self.is_at_end() && self.current_kind() == TokenKind::RightBrace {
                self.advance(); // skip }
            }
        }

        ClassMember::Enum {
            name,
            name_span,
            members: enum_members,
            span,
        }
    }

    fn parse_const(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip const

        let (name, name_span) = self.expect_identifier();
        let type_hint = self.try_parse_type_hint();

        // Skip the rest (= value).
        self.skip_to_newline();

        ClassMember::Constant {
            name,
            name_span,
            type_hint,
            span,
        }
    }

    fn parse_static_member(&mut self, annotations: Vec<AnnotationInfo>) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip static

        match self.current_kind() {
            TokenKind::Var => {
                self.advance(); // skip var
                let (name, name_span) = self.expect_identifier();
                let type_hint = self.try_parse_type_hint();
                self.skip_to_newline();

                ClassMember::StaticVariable {
                    name,
                    name_span,
                    type_hint,
                    annotations,
                    span,
                }
            }
            TokenKind::Func => self.parse_function(true, annotations),
            _ => {
                // Unknown static member, skip.
                self.skip_to_newline();
                ClassMember::StaticVariable {
                    name: String::new(),
                    name_span: Span::new(0, 0, 0, 0),
                    type_hint: None,
                    annotations,
                    span,
                }
            }
        }
    }

    fn parse_variable(&mut self, annotations: Vec<AnnotationInfo>) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip var

        let (name, name_span) = self.expect_identifier();
        let type_hint = self.try_parse_type_hint();

        // Track whether the signature ends in `:` (computed property
        // shorthand) before we consume the rest of the line: in that case we
        // need to consume the indented get/set block too, so the outer parse
        // loop doesn't trip on stray Indent/Dedent tokens.
        let mut ends_with_colon = false;
        while !self.is_at_end()
            && self.current_kind() != TokenKind::Newline
            && self.current_kind() != TokenKind::Eof
        {
            ends_with_colon = matches!(self.current_kind(), TokenKind::Colon);
            self.advance();
        }

        if ends_with_colon {
            self.skip_indented_block();
        }

        ClassMember::Variable {
            name,
            name_span,
            type_hint,
            annotations,
            span,
        }
    }

    /// Skip an optional indented block following a `:`-terminated line. Walks
    /// past the leading Newline+Indent pair, then consumes tokens until the
    /// matching Dedent. Has no effect if no Indent follows.
    fn skip_indented_block(&mut self) {
        // Walk past any blank Newlines.
        while !self.is_at_end() && self.current_kind() == TokenKind::Newline {
            self.advance();
        }
        if self.is_at_end() || self.current_kind() != TokenKind::Indent {
            return;
        }
        self.advance(); // skip Indent
        let mut depth: usize = 1;
        while !self.is_at_end() && depth > 0 {
            match self.current_kind() {
                TokenKind::Indent => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Dedent => {
                    depth -= 1;
                    self.advance();
                }
                _ => self.advance(),
            }
        }
    }

    fn parse_function(&mut self, is_static: bool, annotations: Vec<AnnotationInfo>) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip func

        let (name, name_span) = self.expect_identifier();
        let parameters = if !self.is_at_end() && self.current_kind() == TokenKind::LeftParen {
            self.parse_parameter_list()
        } else {
            Vec::new()
        };

        let return_type = if !self.is_at_end() && self.current_kind() == TokenKind::Arrow {
            self.advance(); // skip ->
            Some(self.expect_type_name())
        } else {
            None
        };

        // Skip to colon.
        while !self.is_at_end()
            && self.current_kind() != TokenKind::Colon
            && self.current_kind() != TokenKind::Newline
        {
            self.advance();
        }
        if !self.is_at_end() && self.current_kind() == TokenKind::Colon {
            self.advance(); // skip :
        }

        // Count body lines (between Indent and Dedent).
        let body_line_count = self.count_body_lines();

        ClassMember::Function {
            name,
            name_span,
            parameters,
            return_type,
            is_static,
            annotations,
            body_line_count,
            span,
        }
    }

    fn parse_inner_class(&mut self) -> ClassMember {
        let span = self.current_token().span;
        self.advance(); // skip class

        let (name, name_span) = self.expect_identifier();

        // Skip extends if present.
        if !self.is_at_end() && self.current_kind() == TokenKind::Extends {
            self.advance();
            let _ = self.expect_identifier(); // base class name
        }

        // Skip colon.
        if !self.is_at_end() && self.current_kind() == TokenKind::Colon {
            self.advance();
        }

        // Parse inner class body (between Indent and Dedent).
        let members = self.parse_indented_block();

        ClassMember::InnerClass {
            name,
            name_span,
            members,
            span,
        }
    }

    fn parse_parameter_list(&mut self) -> Vec<Parameter> {
        let mut params = Vec::new();
        self.advance(); // skip (

        while !self.is_at_end() && self.current_kind() != TokenKind::RightParen {
            self.skip_newlines();
            if self.is_at_end() || self.current_kind() == TokenKind::RightParen {
                break;
            }

            let (name, param_span) = self.expect_identifier();
            if name.is_empty() {
                // Recovery: skip to next comma or close paren.
                while !self.is_at_end()
                    && self.current_kind() != TokenKind::Comma
                    && self.current_kind() != TokenKind::RightParen
                {
                    self.advance();
                }
                if !self.is_at_end() && self.current_kind() == TokenKind::Comma {
                    self.advance();
                }
                continue;
            }

            let type_hint = self.try_parse_type_hint();

            // Skip default value.
            if !self.is_at_end() && self.current_kind() == TokenKind::Assign {
                self.advance(); // skip =
                self.skip_expression();
            }

            params.push(Parameter {
                name,
                type_hint,
                span: param_span,
            });

            if !self.is_at_end() && self.current_kind() == TokenKind::Comma {
                self.advance();
            }
        }

        if !self.is_at_end() && self.current_kind() == TokenKind::RightParen {
            self.advance(); // skip )
        }

        params
    }

    fn try_parse_type_hint(&mut self) -> Option<String> {
        if !self.is_at_end() && self.current_kind() == TokenKind::Colon {
            self.advance(); // skip :

            // Check for := (type inference).
            if !self.is_at_end() && self.current_kind() == TokenKind::Assign {
                return Some(":=".to_string());
            }

            Some(self.expect_type_name())
        } else {
            None
        }
    }

    fn expect_type_name(&mut self) -> String {
        let mut name = String::new();

        if self.is_at_end() {
            return name;
        }

        match self.current_kind() {
            TokenKind::Void => {
                name = "void".to_string();
                self.advance();
            }
            TokenKind::Identifier(ref n) => {
                name = n.clone();
                self.advance();
                // Handle Array[Type] and Dictionary[Key, Value]. Reconstruct
                // the hint with canonical spacing (`Dictionary[int, String]`)
                // rather than concatenating raw token text, which would
                // produce `Dictionary[int,String]`.
                if !self.is_at_end() && self.current_kind() == TokenKind::LeftBracket {
                    name.push('[');
                    self.advance();
                    let mut depth = 1;
                    while !self.is_at_end() && depth > 0 {
                        match self.current_kind() {
                            TokenKind::LeftBracket => {
                                name.push('[');
                                depth += 1;
                            }
                            TokenKind::RightBracket => {
                                depth -= 1;
                                if depth > 0 {
                                    name.push(']');
                                }
                            }
                            TokenKind::Comma => {
                                name.push_str(", ");
                            }
                            _ => {
                                name.push_str(&self.current_token().text);
                            }
                        }
                        self.advance();
                    }
                    name.push(']');
                }
            }
            _ => {}
        }

        name
    }

    fn expect_identifier(&mut self) -> (String, Span) {
        if self.is_at_end() {
            return (String::new(), Span::new(0, 0, 0, 0));
        }
        if let TokenKind::Identifier(ref name) = self.current_kind() {
            let n = name.clone();
            let s = self.current_token().span;
            self.advance();
            (n, s)
        } else {
            (String::new(), Span::new(0, 0, 0, 0))
        }
    }

    fn count_body_lines(&mut self) -> usize {
        // Look for Indent token, skipping newlines and comments that may appear
        // between the colon and the indented body (the lexer emits comments
        // before Indent tokens, so we must skip them here).
        self.skip_newlines_and_comments();
        if self.is_at_end() || self.current_kind() != TokenKind::Indent {
            return 0;
        }

        let mut depth = 1;
        self.advance(); // skip Indent

        let start_line = if !self.is_at_end() {
            self.current_token().span.line
        } else {
            return 0;
        };
        let mut end_line = start_line;

        while !self.is_at_end() && depth > 0 {
            // The lexer emits `##` doc comments and `#` comments at the
            // physical column they appear on, which means a comment placed
            // between this body and the next top-level declaration shows up
            // here BEFORE the closing Dedent. If that's the case, stop now
            // and leave the comment for the outer parser: it logically
            // belongs to whatever comes after the body, not to the body.
            if matches!(
                self.current_kind(),
                TokenKind::DocComment(_) | TokenKind::Comment(_)
            ) && depth == 1
            {
                let mut peek = self.position;
                while peek < self.tokens.len() {
                    match &self.tokens[peek].kind {
                        TokenKind::Newline
                        | TokenKind::DocComment(_)
                        | TokenKind::Comment(_) => peek += 1,
                        TokenKind::Dedent => {
                            return end_line.saturating_sub(start_line) + 1;
                        }
                        _ => break,
                    }
                }
            }

            match self.current_kind() {
                TokenKind::Indent => depth += 1,
                TokenKind::Dedent => depth -= 1,
                _ => {
                    let line = self.current_token().span.line;
                    if line > end_line {
                        end_line = line;
                    }
                }
            }
            if depth > 0 {
                self.advance();
            }
        }

        if !self.is_at_end() && self.current_kind() == TokenKind::Dedent {
            self.advance(); // skip final Dedent
        }

        end_line.saturating_sub(start_line) + 1
    }

    fn parse_indented_block(&mut self) -> Vec<ClassMember> {
        self.skip_newlines();
        if self.is_at_end() || self.current_kind() != TokenKind::Indent {
            return Vec::new();
        }

        self.advance(); // skip Indent

        let mut members = Vec::new();
        let mut pending_annotations: Vec<AnnotationInfo> = Vec::new();

        while !self.is_at_end() && self.current_kind() != TokenKind::Dedent {
            self.skip_newlines();
            if self.is_at_end() || self.current_kind() == TokenKind::Dedent {
                break;
            }

            match self.current_kind() {
                TokenKind::Annotation(_) => {
                    let annotation = self.parse_annotation();
                    self.skip_annotation_args();
                    pending_annotations.push(annotation);
                }
                TokenKind::Signal => {
                    members.push(self.parse_signal());
                }
                TokenKind::Enum => {
                    members.push(self.parse_enum());
                }
                TokenKind::Const => {
                    members.push(self.parse_const());
                }
                TokenKind::Var => {
                    let annotations = std::mem::take(&mut pending_annotations);
                    members.push(self.parse_variable(annotations));
                }
                TokenKind::Static => {
                    let annotations = std::mem::take(&mut pending_annotations);
                    members.push(self.parse_static_member(annotations));
                }
                TokenKind::Func => {
                    let annotations = std::mem::take(&mut pending_annotations);
                    members.push(self.parse_function(false, annotations));
                }
                TokenKind::DocComment(_) => {
                    members.push(self.parse_doc_comment());
                }
                _ => {
                    self.advance();
                }
            }
        }

        if !self.is_at_end() && self.current_kind() == TokenKind::Dedent {
            self.advance();
        }

        members
    }

    fn skip_newlines(&mut self) {
        while !self.is_at_end() && self.current_kind() == TokenKind::Newline {
            self.advance();
        }
    }

    fn skip_newlines_and_comments(&mut self) {
        while !self.is_at_end() {
            match self.current_kind() {
                TokenKind::Newline => self.advance(),
                TokenKind::Comment(_) | TokenKind::DocComment(_) => self.advance(),
                _ => break,
            }
        }
    }

    fn skip_to_newline(&mut self) {
        while !self.is_at_end()
            && self.current_kind() != TokenKind::Newline
            && self.current_kind() != TokenKind::Eof
        {
            self.advance();
        }
    }

    fn skip_expression(&mut self) {
        let mut paren_depth = 0;
        while !self.is_at_end() {
            match self.current_kind() {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                    paren_depth += 1;
                    self.advance();
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                    if paren_depth == 0 {
                        break;
                    }
                    paren_depth -= 1;
                    self.advance();
                }
                TokenKind::Comma if paren_depth == 0 => break,
                TokenKind::Newline if paren_depth == 0 => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn current_token(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn current_kind(&self) -> TokenKind {
        self.tokens[self.position].kind.clone()
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len() || self.tokens[self.position].kind == TokenKind::Eof
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse_source(source: &str) -> Vec<ClassMember> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        Parser::new(&tokens).parse()
    }

    #[test]
    fn parse_class_name_and_extends() {
        let members = parse_source("class_name Player\nextends CharacterBody2D\n");
        assert!(
            matches!(members[0], ClassMember::ClassNameDecl { ref name, .. } if name == "Player")
        );
        assert!(
            matches!(members[1], ClassMember::ExtendsDecl { ref base, .. } if base == "CharacterBody2D")
        );
    }

    #[test]
    fn parse_signal_without_params() {
        let members = parse_source("signal player_died\n");
        assert!(
            matches!(members[0], ClassMember::Signal { ref name, ref parameters, .. }
            if name == "player_died" && parameters.is_empty())
        );
    }

    #[test]
    fn parse_signal_with_params() {
        let members = parse_source("signal health_changed(old_value: int, new_value: int)\n");
        if let ClassMember::Signal {
            ref name,
            ref parameters,
            ..
        } = members[0]
        {
            assert_eq!(name, "health_changed");
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name, "old_value");
            assert_eq!(parameters[1].name, "new_value");
        } else {
            panic!("expected Signal");
        }
    }

    #[test]
    fn parse_enum() {
        let members = parse_source("enum State { IDLE, WALKING, RUNNING }\n");
        if let ClassMember::Enum {
            ref name,
            ref members,
            ..
        } = members[0]
        {
            assert_eq!(name.as_deref(), Some("State"));
            assert_eq!(members.len(), 3);
            assert_eq!(members[0].name, "IDLE");
            assert_eq!(members[1].name, "WALKING");
            assert_eq!(members[2].name, "RUNNING");
        } else {
            panic!("expected Enum");
        }
    }

    #[test]
    fn parse_constant() {
        let members = parse_source("const MAX_SPEED: float = 200.0\n");
        if let ClassMember::Constant {
            ref name,
            ref type_hint,
            ..
        } = members[0]
        {
            assert_eq!(name, "MAX_SPEED");
            assert_eq!(type_hint.as_deref(), Some("float"));
        } else {
            panic!("expected Constant");
        }
    }

    #[test]
    fn parse_typed_collection_hint_has_canonical_spacing() {
        let members = parse_source("var scores: Dictionary[int, String] = {}\n");
        if let ClassMember::Variable { ref type_hint, .. } = members[0] {
            assert_eq!(type_hint.as_deref(), Some("Dictionary[int, String]"));
        } else {
            panic!("expected Variable");
        }
        let arr = parse_source("var xs: Array[Vector2] = []\n");
        if let ClassMember::Variable { ref type_hint, .. } = arr[0] {
            assert_eq!(type_hint.as_deref(), Some("Array[Vector2]"));
        } else {
            panic!("expected Variable");
        }
    }

    #[test]
    fn parse_variable_with_annotation() {
        let members = parse_source("@export var speed: float = 200.0\n");
        if let ClassMember::Variable {
            ref name,
            ref annotations,
            ..
        } = members[0]
        {
            assert_eq!(name, "speed");
            assert!(annotations.iter().any(|a| a.name == "export"));
        } else {
            panic!("expected Variable, got {:?}", members[0]);
        }
    }

    #[test]
    fn parse_onready_variable() {
        let members = parse_source("@onready var label: Label = $Label\n");
        if let ClassMember::Variable {
            ref name,
            ref annotations,
            ..
        } = members[0]
        {
            assert_eq!(name, "label");
            assert!(annotations.iter().any(|a| a.name == "onready"));
        } else {
            panic!("expected Variable");
        }
    }

    #[test]
    fn parse_function() {
        let members = parse_source("func take_damage(amount: int) -> void:\n\tpass\n");
        if let ClassMember::Function {
            ref name,
            ref parameters,
            ref return_type,
            ..
        } = members[0]
        {
            assert_eq!(name, "take_damage");
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "amount");
            assert_eq!(return_type.as_deref(), Some("void"));
        } else {
            panic!("expected Function");
        }
    }

    #[test]
    fn parse_static_function() {
        let members = parse_source("static func create() -> void:\n\tpass\n");
        if let ClassMember::Function {
            ref name,
            is_static,
            ..
        } = members[0]
        {
            assert_eq!(name, "create");
            assert!(is_static);
        } else {
            panic!("expected static Function");
        }
    }

    #[test]
    fn parse_doc_comment() {
        let members = parse_source("## This is a doc comment\n");
        assert!(matches!(members[0], ClassMember::DocComment { .. }));
    }

    #[test]
    fn parse_tool_annotation() {
        let members = parse_source("@tool\n");
        assert!(matches!(members[0], ClassMember::ToolAnnotation { .. }));
    }

    #[test]
    fn parse_ordering_categories() {
        let members = parse_source(
            "\
@tool
class_name Test
extends Node
signal test_signal
enum State { IDLE }
const MAX = 10
static var count: int = 0
@export var speed: float = 1.0
var health: int = 100
@onready var label: Label = $Label
func _ready() -> void:
\tpass
func custom_method() -> void:
\tpass
",
        );

        // Verify ordering categories are ascending.
        let mut last_category = 0;
        for member in &members {
            let cat = member.ordering_category();
            if cat == usize::MAX {
                continue; // Skip comments/blanks.
            }
            assert!(
                cat >= last_category,
                "ordering violation: {} (cat {}) after category {}",
                member.category_name(),
                cat,
                last_category
            );
            last_category = cat;
        }
    }

    #[test]
    fn parse_full_script() {
        let source = r#"@tool
class_name StateMachine
extends Node
## Hierarchical state machine.

signal state_changed(previous: String, new: String)

enum State { IDLE, RUNNING }

const MAX_STATES: int = 10

@export var initial_state: Node

var is_active: bool = true

@onready var _state: Node = $State

func _ready() -> void:
	pass

func transition_to(target: String) -> void:
	pass
"#;
        let members = parse_source(source);
        // Should parse without panics and have at least the expected members.
        let class_name = members
            .iter()
            .find(|m| matches!(m, ClassMember::ClassNameDecl { .. }));
        assert!(class_name.is_some());

        let signals: Vec<_> = members
            .iter()
            .filter(|m| matches!(m, ClassMember::Signal { .. }))
            .collect();
        assert_eq!(signals.len(), 1);

        let functions: Vec<_> = members
            .iter()
            .filter(|m| matches!(m, ClassMember::Function { .. }))
            .collect();
        assert_eq!(functions.len(), 2);
    }
}
