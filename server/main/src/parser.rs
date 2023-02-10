use std::{
    collections::HashMap,
    path::PathBuf
};

use regex::Regex;
use rust_lsp::lsp_types::Diagnostic;
use slog_scope::debug;
use url::Url;

use crate::opengl::{self, ShaderValidator};

pub struct DiagnosticsParser {
    line_offset: i32,
    line_regex: Regex,
}

impl DiagnosticsParser {
    pub fn new(vendor_querier: &opengl::OpenGlContext) -> Self {
        let line_regex = match vendor_querier.vendor().as_str() {
            "NVIDIA Corporation" => {
                Regex::new(r#"^(?P<filepath>\d+)\((?P<linenum>\d+)\) : (?P<severity>error|warning) [A-C]\d+: (?P<output>.+)"#).unwrap()
            }
            _ => Regex::new(r#"^(?P<severity>ERROR|WARNING): (?P<filepath>[^?<>*|"\n]+):(?P<linenum>\d+): (?:'.*' :|[a-z]+\(#\d+\)) +(?P<output>.+)$"#)
                .unwrap(),
        };
        let line_offset = match vendor_querier.vendor().as_str() {
            "ATI Technologies" => 0,
            _ => 1,
        };
        DiagnosticsParser {
            line_offset: line_offset,
            line_regex: line_regex,
        }
    }

    pub fn parse_diagnostics(&self, compile_log: String, files: HashMap<i32, PathBuf>) -> HashMap<Url, Vec<Diagnostic>> {
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        debug!("diagnostics regex selected"; "regex" => &self.line_regex.to_string());

        for line in compile_log.split('\n').collect::<Vec<&str>>() {
            ;
        }

        diagnostics
    }
}