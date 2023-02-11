#![feature(option_get_or_insert_default)]
#![feature(linked_list_cursors)]

use rust_lsp::jsonrpc::{method_types::*, *};
use rust_lsp::lsp::*;
use rust_lsp::lsp_types::{notification::*, *};

use serde::Deserialize;
use serde_json::{from_value, Value};

use tree_sitter::Parser;
use url_norm::FromUrl;

use std::collections::{HashMap, HashSet, LinkedList};
use std::convert::TryFrom;
use std::io::{stdin, stdout};
use std::iter::Extend;
use std::rc::Rc;

use std::{
    cell::RefCell,
    path::{PathBuf},
};

use slog::Level;
use slog_scope::{error, info, warn};

use regex::Regex;

use lazy_static::lazy_static;

mod commands;
mod configuration;
mod linemap;
mod lsp_ext;
mod navigation;
mod opengl;
mod parser;
mod shaders;
mod url_norm;

lazy_static! {
    static ref RE_DIMENSION_FOLDER: Regex = Regex::new(r#"^world-?\d+"#).unwrap();
    static ref RE_DEFAULT_SHADERS: HashSet<String> = {
        let mut set = HashSet::with_capacity(1716);
        for ext in ["fsh", "vsh", "gsh", "csh"] {
            set.insert(format!("composite.{}", ext));
            set.insert(format!("deferred.{}", ext));
            set.insert(format!("prepare.{}", ext));
            set.insert(format!("shadowcomp.{}", ext));
            for i in 1..=99 {
                let total_suffix = format!("{}.{}", i, ext);
                set.insert(format!("composite{}", total_suffix));
                set.insert(format!("deferred{}", total_suffix));
                set.insert(format!("prepare{}", total_suffix));
                set.insert(format!("shadowcomp{}", total_suffix));
            }
            set.insert(format!("composite_pre.{}", ext));
            set.insert(format!("deferred_pre.{}", ext));
            set.insert(format!("final.{}", ext));
            set.insert(format!("gbuffers_armor_glint.{}", ext));
            set.insert(format!("gbuffers_basic.{}", ext));
            set.insert(format!("gbuffers_beaconbeam.{}", ext));
            set.insert(format!("gbuffers_block.{}", ext));
            set.insert(format!("gbuffers_clouds.{}", ext));
            set.insert(format!("gbuffers_damagedblock.{}", ext));
            set.insert(format!("gbuffers_entities.{}", ext));
            set.insert(format!("gbuffers_entities_glowing.{}", ext));
            set.insert(format!("gbuffers_hand.{}", ext));
            set.insert(format!("gbuffers_hand_water.{}", ext));
            set.insert(format!("gbuffers_item.{}", ext));
            set.insert(format!("gbuffers_line.{}", ext));
            set.insert(format!("gbuffers_skybasic.{}", ext));
            set.insert(format!("gbuffers_skytextured.{}", ext));
            set.insert(format!("gbuffers_spidereyes.{}", ext));
            set.insert(format!("gbuffers_terrain.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout_mip.{}", ext));
            set.insert(format!("gbuffers_terrain_solid.{}", ext));
            set.insert(format!("gbuffers_textured.{}", ext));
            set.insert(format!("gbuffers_textured_lit.{}", ext));
            set.insert(format!("gbuffers_water.{}", ext));
            set.insert(format!("gbuffers_weather.{}", ext));
            set.insert(format!("shadow.{}", ext));
            set.insert(format!("shadow_cutout.{}", ext));
            set.insert(format!("shadow_solid.{}", ext));
        }
        let base_char_num = 'a' as u8;
        for suffix_num in 0u8..=25u8 {
            let suffix_char = (base_char_num + suffix_num) as char;
            set.insert(format!("composite_{}.csh", suffix_char));
            set.insert(format!("deferred_{}.csh", suffix_char));
            set.insert(format!("prepare_{}.csh", suffix_char));
            set.insert(format!("shadowcomp_{}.csh", suffix_char));
            for i in 1..=99 {
                let total_suffix = format!("{}_{}", i, suffix_char);
                set.insert(format!("composite{}.csh", total_suffix));
                set.insert(format!("deferred{}.csh", total_suffix));
                set.insert(format!("prepare{}.csh", total_suffix));
                set.insert(format!("shadowcomp{}.csh", total_suffix));
            }
        }
        set
    };
}

fn main() {
    let guard = logging::set_logger_with_level(Level::Info);

    let endpoint_output = LSPEndpoint::create_lsp_output_with_output_stream(stdout);

    let mut parser = Parser::new();
    parser.set_language(tree_sitter_glsl::language()).unwrap();

    let opengl_context = Rc::new(opengl::OpenGlContext::new());

    let mut langserver = MinecraftShaderLanguageServer {
        endpoint: endpoint_output.clone(),
        root: "".into(),
        command_provider: None,
        opengl_context: opengl_context.clone(),
        tree_sitter: Rc::new(RefCell::new(parser)),
        log_guard: Some(guard),
        shader_files: HashMap::new(),
        include_files: HashMap::new(),
        diagnostics_parser: parser::DiagnosticsParser::new(opengl_context.as_ref()),
    };

    langserver.command_provider = Some(commands::CustomCommandProvider::new(vec![
        (
            "parseTree",
            Box::new(commands::parse_tree::TreeSitterSExpr {
                tree_sitter: langserver.tree_sitter.clone(),
            }),
        ),
    ]));

    LSPEndpoint::run_server_from_input(&mut stdin().lock(), endpoint_output, langserver);
}

pub struct MinecraftShaderLanguageServer {
    endpoint: Endpoint,
    root: PathBuf,
    command_provider: Option<commands::CustomCommandProvider>,
    opengl_context: Rc<dyn opengl::ShaderValidator>,
    tree_sitter: Rc<RefCell<Parser>>,
    log_guard: Option<slog_scope::GlobalLoggerGuard>,
    shader_files: HashMap<PathBuf, shaders::ShaderFile>,
    include_files: HashMap<PathBuf, shaders::IncludeFile>,
    diagnostics_parser: parser::DiagnosticsParser,
}

impl MinecraftShaderLanguageServer {
    pub fn error_not_available<DATA>(data: DATA) -> MethodError<DATA> {
        let msg = "Functionality not implemented.".to_string();
        MethodError::<DATA> {
            code: 1,
            message: msg,
            data,
        }
    }

    fn find_work_space(&self, curr_path: &PathBuf) -> HashSet<PathBuf> {
        let mut work_spaces: HashSet<PathBuf> = HashSet::new();
        for file in curr_path.read_dir().expect("read directory failed") {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    let file_name = file_path.file_name().unwrap();
                    if file_name == "shaders" {
                        info!("find work space {}", &file_path.to_str().unwrap());
                        work_spaces.insert(file_path);
                    }
                    else {
                        work_spaces.extend(self.find_work_space(&file_path));
                    }
                }
            }
        }
        work_spaces
    }

    fn add_shader_file(&mut self, work_space: &PathBuf, file_path: PathBuf) {
        if RE_DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
            let mut shader_file = shaders::ShaderFile::new(work_space, &file_path);
            shader_file.read_file(&mut self.include_files);
            self.shader_files.insert(file_path, shader_file);
        }
    }

    fn remove_shader_file(&mut self, file_path: &PathBuf) {
        self.shader_files.remove(file_path);
        for include_file in &mut self.include_files {
            let included_shaders = include_file.1.included_shaders_mut();
            if included_shaders.contains(file_path) {
                included_shaders.remove(file_path);
            }
        }
    }

    fn build_file_framework(&mut self) {
        info!("generating file framework on current root"; "root" => self.root.to_str().unwrap());

        let work_spaces: HashSet<PathBuf> = self.find_work_space(&self.root);
        for work_space in &work_spaces {
            for file in work_space.read_dir().expect("read work space failed") {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() {
                        self.add_shader_file(work_space, file_path);
                    }
                    else if file_path.is_dir() && RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() {
                                    self.add_shader_file(work_space, file_path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn update_file(&mut self, path: &PathBuf) {
        if self.shader_files.contains_key(path) {
            let shader_file = self.shader_files.get_mut(path).unwrap();
            shader_file.clear_including_files();
            shader_file.read_file(&mut self.include_files);
        }
        if self.include_files.contains_key(path) {
            let mut include_file = self.include_files.remove(path).unwrap();
            include_file.update_include(&mut self.include_files);
            self.include_files.insert(path.clone(), include_file);
        }
    }

    fn lint_shader(&mut self, path: &PathBuf) -> HashMap<Url, Vec<Diagnostic>> {
        if !path.exists() {
            self.remove_shader_file(path);
            return HashMap::new();
        }
        let shader_file = self.shader_files.get(path).unwrap();

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = shader_file.merge_shader_file(&self.include_files, &mut file_list);

        let validation_result = self.opengl_context.validate_shader(shader_file.file_type(), &shader_content);

        // Copied from original file
        match &validation_result {
            Some(output) => {
                info!("compilation errors reported"; "errors" => format!("`{}`", output.replace('\n', "\\n")), "tree_root" => path.to_str().unwrap())
            }
            None => {
                info!("compilation reported no errors"; "tree_root" => path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(path).unwrap()).or_default();
                for include_file in shader_file.including_files() {
                    diagnostics.entry(Url::from_file_path(&include_file.3).unwrap()).or_default();
                }
                return diagnostics;
            },
        };

        self.diagnostics_parser.parse_diagnostics(validation_result.unwrap(), file_list)
    }

    fn update_lint(&mut self, path: &PathBuf) {
        self.set_status("loading", "Compiling shaders...", "$(loading~spin)");
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        if self.shader_files.contains_key(path) {
            diagnostics.extend(self.lint_shader(path));
        }
        if self.include_files.contains_key(path) {
            let shader_files = self.include_files.get(path).unwrap();
            for shader_path in shader_files.included_shaders().clone() {
                diagnostics.extend(self.lint_shader(&shader_path));
            }
        }
        self.publish_diagnostic(diagnostics, None);
        self.set_status("ready", "Compiled all changed shaders", "$(check)");
    }

    pub fn publish_diagnostic(&self, diagnostics: HashMap<Url, Vec<Diagnostic>>, document_version: Option<i32>) {
        // info!("DIAGNOSTICS:\n{:?}", diagnostics);
        for (uri, diagnostics) in diagnostics {
            self.endpoint
                .send_notification(
                    PublishDiagnostics::METHOD,
                    PublishDiagnosticsParams {
                        uri,
                        diagnostics,
                        version: document_version,
                    },
                )
                .expect("failed to publish diagnostics");
        }
    }

    fn set_status(&self, status: impl Into<String>, message: impl Into<String>, icon: impl Into<String>) {
        self.endpoint
            .send_notification(
                lsp_ext::Status::METHOD,
                lsp_ext::StatusParams {
                    status: status.into(),
                    message: Some(message.into()),
                    icon: Some(icon.into()),
                },
            )
            .unwrap_or(());
    }
}

impl LanguageServerHandling for MinecraftShaderLanguageServer {
    fn initialize(&mut self, params: InitializeParams, completable: MethodCompletable<InitializeResult, InitializeError>) {
        logging::slog_with_trace_id(|| {
            info!("starting server...");

            let capabilities = ServerCapabilities {
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: None,
                    work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["graphDot".into()],
                    work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    open_close: Some(true),
                    will_save: None,
                    will_save_wait_until: None,
                    change: Some(TextDocumentSyncKind::FULL),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions { include_text: Some(true) })),
                })),
                ..ServerCapabilities::default()
            };

            let root = match params.root_uri {
                Some(uri) => PathBuf::from_url(uri),
                None => {
                    completable.complete(Err(MethodError {
                        code: 42069,
                        message: "Must be in workspace".into(),
                        data: InitializeError { retry: false },
                    }));
                    return;
                }
            };

            completable.complete(Ok(InitializeResult {
                capabilities,
                server_info: None,
            }));

            self.set_status("loading", "Building file framework...", "$(loading~spin)");

            self.root = root;

            self.build_file_framework();

            self.set_status("ready", "Project initialized", "$(check)");
        });
    }

    fn shutdown(&mut self, _: (), completable: LSCompletable<()>) {
        warn!("shutting down language server...");
        completable.complete(Ok(()));
    }

    fn exit(&mut self, _: ()) {
        self.endpoint.request_shutdown();
    }

    fn workspace_change_configuration(&mut self, params: DidChangeConfigurationParams) {
        logging::slog_with_trace_id(|| {
            #[derive(Deserialize)]
            struct Configuration {
                #[serde(alias = "logLevel")]
                log_level: String,
            }

            if let Some(settings) = params.settings.as_object().unwrap().get("mcglsl") {
                let config: Configuration = from_value(settings.to_owned()).unwrap();

                info!("got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcglsl").unwrap().to_string());

                configuration::handle_log_level_change(config.log_level, |level| {
                    self.log_guard = None; // set to None so Drop is invoked
                    self.log_guard = Some(logging::set_logger_with_level(level));
                })
            }
        });
    }

    fn did_open_text_document(&mut self, params: DidOpenTextDocumentParams) {
        logging::slog_with_trace_id(|| {
            //info!("opened doc {}", params.text_document.uri);
            let path = PathBuf::from_url(params.text_document.uri);
            self.update_lint(&path);
        });
    }

    fn did_change_text_document(&mut self, _: DidChangeTextDocumentParams) {}

    fn did_close_text_document(&mut self, _: DidCloseTextDocumentParams) {}

    fn did_save_text_document(&mut self, params: DidSaveTextDocumentParams) {
        logging::slog_with_trace_id(|| {
            let path = PathBuf::from_url(params.text_document.uri);
            self.update_file(&path);
            self.update_lint(&path);
        });
    }

    fn did_change_watched_files(&mut self, params: DidChangeWatchedFilesParams) {
        logging::slog_with_trace_id(|| {
            // Collect all shaders in changed include files into this set
            // This can avoid linting duplicate shaders
            let mut updated_shaders: HashSet<PathBuf> = HashSet::new();
            for change in params.changes {
                let path = PathBuf::from_url(change.uri);
                if change.typ == FileChangeType::DELETED {
                    if self.shader_files.contains_key(&path) {
                        self.remove_shader_file(&path);
                    }
                }
                else if self.shader_files.contains_key(&path){
                    self.update_file(&path);
                    updated_shaders.insert(path);
                }
                else if self.include_files.contains_key(&path) {
                    self.update_file(&path);
                    updated_shaders.extend(self.include_files.get(&path).unwrap().included_shaders().clone());
                }
            }
            // Lint all collected parent
            for shader in updated_shaders {
                // We are sure that all pathes are shader files but not include files
                self.lint_shader(&shader);
            }
        })
    }

    fn completion(&mut self, _: TextDocumentPositionParams, completable: LSCompletable<CompletionList>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn resolve_completion_item(&mut self, _: CompletionItem, completable: LSCompletable<CompletionItem>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn hover(&mut self, _: TextDocumentPositionParams, _: LSCompletable<Hover>) {
        /* completable.complete(Ok(Hover{
            contents: HoverContents::Markup(MarkupContent{
                kind: MarkupKind::Markdown,
                value: String::from("# Hello World"),
            }),
            range: None,
        })); */
    }

    fn execute_command(&mut self, params: ExecuteCommandParams, completable: LSCompletable<Option<Value>>) {
        logging::slog_with_trace_id(|| {
            match self
                .command_provider
                .as_ref()
                .unwrap()
                .execute(&params.command, &params.arguments, &self.root)
            {
                Ok(resp) => {
                    info!("executed command successfully"; "command" => params.command.clone());
                    self.endpoint
                        .send_notification(
                            ShowMessage::METHOD,
                            ShowMessageParams {
                                typ: MessageType::INFO,
                                message: format!("Command {} executed successfully.", params.command),
                            },
                        )
                        .expect("failed to send popup/show message notification");
                    completable.complete(Ok(Some(resp)))
                }
                Err(err) => {
                    error!("failed to execute command"; "command" => params.command.clone(), "error" => format!("{:?}", err));
                    self.endpoint
                        .send_notification(
                            ShowMessage::METHOD,
                            ShowMessageParams {
                                typ: MessageType::ERROR,
                                message: format!("Failed to execute `{}`. Reason: {}", params.command, err),
                            },
                        )
                        .expect("failed to send popup/show message notification");
                    completable.complete(Err(MethodError::new(32420, err.to_string(), ())))
                }
            }
        });
    }

    fn signature_help(&mut self, _: TextDocumentPositionParams, completable: LSCompletable<SignatureHelp>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn goto_definition(&mut self, params: TextDocumentPositionParams, completable: LSCompletable<Vec<Location>>) {
        logging::slog_with_trace_id(|| {
            let path = PathBuf::from_url(params.text_document.uri);
            if !path.starts_with(&self.root) {
                return;
            }
            let parser = &mut self.tree_sitter.borrow_mut();
            let parser_ctx = match navigation::ParserContext::new(parser, &path) {
                Ok(ctx) => ctx,
                Err(e) => {
                    return completable.complete(Err(MethodError {
                        code: 42069,
                        message: format!("error building parser context: error={}, path={:?}", e, path),
                        data: (),
                    }))
                }
            };

            match parser_ctx.find_definitions(&path, params.position) {
                Ok(locations) => completable.complete(Ok(locations.unwrap_or_default())),
                Err(e) => completable.complete(Err(MethodError {
                    code: 42069,
                    message: format!("error finding definitions: error={}, path={:?}", e, path),
                    data: (),
                })),
            }
        });
    }

    fn references(&mut self, params: ReferenceParams, completable: LSCompletable<Vec<Location>>) {
        logging::slog_with_trace_id(|| {
            let path = PathBuf::from_url(params.text_document_position.text_document.uri);
            if !path.starts_with(&self.root) {
                return;
            }
            let parser = &mut self.tree_sitter.borrow_mut();
            let parser_ctx = match navigation::ParserContext::new(parser, &path) {
                Ok(ctx) => ctx,
                Err(e) => {
                    return completable.complete(Err(MethodError {
                        code: 42069,
                        message: format!("error building parser context: error={}, path={:?}", e, path),
                        data: (),
                    }))
                }
            };

            match parser_ctx.find_references(&path, params.text_document_position.position) {
                Ok(locations) => completable.complete(Ok(locations.unwrap_or_default())),
                Err(e) => completable.complete(Err(MethodError {
                    code: 42069,
                    message: format!("error finding definitions: error={}, path={:?}", e, path),
                    data: (),
                })),
            }
        });
    }

    fn document_highlight(&mut self, _: TextDocumentPositionParams, completable: LSCompletable<Vec<DocumentHighlight>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn document_symbols(&mut self, params: DocumentSymbolParams, completable: LSCompletable<DocumentSymbolResponse>) {
        logging::slog_with_trace_id(|| {
            let path = PathBuf::from_url(params.text_document.uri);
            if !path.starts_with(&self.root) {
                return;
            }
            let parser = &mut self.tree_sitter.borrow_mut();
            let parser_ctx = match navigation::ParserContext::new(parser, &path) {
                Ok(ctx) => ctx,
                Err(e) => {
                    return completable.complete(Err(MethodError {
                        code: 42069,
                        message: format!("error building parser context: error={}, path={:?}", e, path),
                        data: (),
                    }))
                }
            };

            match parser_ctx.list_symbols(&path) {
                Ok(symbols) => completable.complete(Ok(DocumentSymbolResponse::from(symbols.unwrap_or_default()))),
                Err(e) => {
                    return completable.complete(Err(MethodError {
                        code: 42069,
                        message: format!("error finding definitions: error={}, path={:?}", e, path),
                        data: (),
                    }))
                }
            }
        });
    }

    fn workspace_symbols(&mut self, _: WorkspaceSymbolParams, completable: LSCompletable<DocumentSymbolResponse>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn code_action(&mut self, _: CodeActionParams, completable: LSCompletable<Vec<Command>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn code_lens(&mut self, _: CodeLensParams, completable: LSCompletable<Vec<CodeLens>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn code_lens_resolve(&mut self, _: CodeLens, completable: LSCompletable<CodeLens>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn document_link(&mut self, params: DocumentLinkParams, completable: LSCompletable<Vec<DocumentLink>>) {
        logging::slog_with_trace_id(|| {
            // Current document path
            let curr_doc = PathBuf::from_url(params.text_document.uri);

            let include_list: &LinkedList<(usize, usize, usize, PathBuf)>;
            if self.shader_files.contains_key(&curr_doc) {
                include_list = self.shader_files.get(&curr_doc).unwrap().including_files();
            }
            else if self.include_files.contains_key(&curr_doc) {
                include_list = self.include_files.get(&curr_doc).unwrap().including_files();
            }
            else {
                warn!("document not found in file system"; "path" => curr_doc.to_str().unwrap());
                completable.complete(Ok(vec![]));
                return;
            }

            let include_links = include_list
                .iter()
                .map(|include_file| {
                    let path = &include_file.3;
                    let url = Url::from_file_path(path).unwrap();
                    DocumentLink {
                        range: Range::new(
                            Position::new(u32::try_from(include_file.0).unwrap(), u32::try_from(include_file.1).unwrap()),
                            Position::new(u32::try_from(include_file.0).unwrap(), u32::try_from(include_file.2).unwrap()),
                        ),
                        tooltip: Some(url.path().to_string()),
                        target: Some(url),
                        data: None,
                    }
                })
                .collect();

            completable.complete(Ok(include_links));
        });
    }

    fn document_link_resolve(&mut self, _: DocumentLink, completable: LSCompletable<DocumentLink>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn formatting(&mut self, _: DocumentFormattingParams, completable: LSCompletable<Vec<TextEdit>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn range_formatting(&mut self, _: DocumentRangeFormattingParams, completable: LSCompletable<Vec<TextEdit>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn on_type_formatting(&mut self, _: DocumentOnTypeFormattingParams, completable: LSCompletable<Vec<TextEdit>>) {
        completable.complete(Err(Self::error_not_available(())));
    }

    fn rename(&mut self, _: RenameParams, completable: LSCompletable<WorkspaceEdit>) {
        completable.complete(Err(Self::error_not_available(())));
    }
}
