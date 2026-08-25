//! Safe bounded content templates.

use crate::path_template::{
    Expression, TemplateContext, TemplateError, parse_expressions, render_source,
};

/// A validated text template without arbitrary code execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentTemplate {
    source: String,
    expressions: Vec<Expression>,
}
impl ContentTemplate {
    /// Parses a template using the documented variable and filter allowlist.
    pub fn new(source: impl Into<String>) -> Result<Self, TemplateError> {
        let source = source.into();
        let expressions = parse_expressions(&source)?;
        Ok(Self {
            source,
            expressions,
        })
    }
    /// Renders text from a validated context.
    pub fn render(&self, context: &TemplateContext) -> Result<String, TemplateError> {
        render_source(&self.source, &self.expressions, context)
    }
}
