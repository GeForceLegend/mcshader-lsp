use super::*;
use std::fs;
use std::io;
use std::io::Result;

use pretty_assertions::assert_eq;

use tempdir::TempDir;

use fs_extra::{copy_items, dir};

use jsonrpc_common::*;
use jsonrpc_response::*;

struct StdoutNewline {
    s: Box<dyn io::Write>,
}

impl io::Write for StdoutNewline {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let res = self.s.write(buf);
        if buf[buf.len() - 1] == b"}"[0] {
            #[allow(unused_variables)]
            let res = self.s.write(b"\n\n");
        }
        res
    }

    fn flush(&mut self) -> Result<()> {
        self.s.flush()
    }
}

pub fn new_temp_server(opengl_context: Option<Box<dyn opengl::ShaderValidator>>) -> MinecraftShaderLanguageServer {
    let endpoint = LSPEndpoint::create_lsp_output_with_output_stream(|| StdoutNewline { s: Box::new(io::sink()) });

    let context = opengl_context.unwrap_or_else(|| Box::new(opengl::MockShaderValidator::new()));

    let ogl_context = Rc::new(opengl::OpenGlContext::new());

    MinecraftShaderLanguageServer {
        endpoint,
        graph: Rc::new(RefCell::new(graph::CachedStableGraph::new())),
        root: "".into(),
        command_provider: None,
        opengl_context: context.into(),
        log_guard: None,
        tree_sitter: Rc::new(RefCell::new(Parser::new())),
        file_extensions: HashSet::new(),
        shader_files: HashMap::new(),
        include_files: HashMap::new(),
        diagnostics_parser: parser::DiagnosticsParser::new(ogl_context.as_ref()),
    }
}

fn copy_files(files: &str, dest: &TempDir) {
    let opts = &dir::CopyOptions::new();
    let files = fs::read_dir(files)
        .unwrap()
        .map(|e| String::from(e.unwrap().path().to_str().unwrap()))
        .collect::<Vec<String>>();
    copy_items(&files, dest.path().join("shaders"), opts).unwrap();
}

fn copy_to_tmp_dir(test_path: &str) -> (Rc<TempDir>, PathBuf) {
    let tmp_dir = Rc::new(TempDir::new("mcshader").unwrap());
    fs::create_dir(tmp_dir.path().join("shaders")).unwrap();

    copy_files(test_path, &tmp_dir);

    let tmp_clone = tmp_dir.clone();
    let tmp_path = tmp_clone.path().to_str().unwrap();

    (tmp_dir, tmp_path.into())
}

#[allow(deprecated)]
#[test]
#[logging_macro::log_scope]
fn test_empty_initialize() {
    let mut server = new_temp_server(None);

    let tmp_dir = TempDir::new("mcshader").unwrap();
    let tmp_path = tmp_dir.path();

    let initialize_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::from_directory_path(tmp_path).unwrap()),
        client_info: None,
        initialization_options: None,
        capabilities: ClientCapabilities {
            workspace: None,
            text_document: None,
            experimental: None,
            window: None,
            general: Option::None,
        },
        trace: None,
        workspace_folders: None,
        locale: Option::None,
    };

    let on_response = |resp: Option<Response>| {
        assert!(resp.is_some());
        let respu = resp.unwrap();
        match respu.result_or_error {
            ResponseResult::Result(_) => {}
            ResponseResult::Error(e) => {
                panic!("expected ResponseResult::Result(..), got {:?}", e)
            }
        }
    };

    let completable = MethodCompletable::new(ResponseCompletable::new(Some(Id::Number(1)), Box::new(on_response)));
    server.initialize(initialize_params, completable);

    assert_eq!(server.root, tmp_path);

    assert_eq!(server.graph.borrow().graph.edge_count(), 0);
    assert_eq!(server.graph.borrow().graph.node_count(), 0);

    server.endpoint.request_shutdown();
}

#[allow(deprecated)]
#[test]
#[logging_macro::log_scope]
fn test_01_initialize() {
    let mut server = new_temp_server(None);

    let (_tmp_dir, tmp_path) = copy_to_tmp_dir("./testdata/01");

    let initialize_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::from_directory_path(tmp_path.clone()).unwrap()),
        client_info: None,
        initialization_options: None,
        capabilities: ClientCapabilities {
            workspace: None,
            text_document: None,
            experimental: None,
            window: None,
            general: Option::None,
        },
        trace: None,
        workspace_folders: None,
        locale: Option::None,
    };

    let on_response = |resp: Option<Response>| {
        assert!(resp.is_some());
        let respu = resp.unwrap();
        match respu.result_or_error {
            ResponseResult::Result(_) => {}
            ResponseResult::Error(e) => {
                panic!("expected ResponseResult::Result(..), got {:?}", e)
            }
        }
    };

    let completable = MethodCompletable::new(ResponseCompletable::new(Some(Id::Number(1)), Box::new(on_response)));
    server.initialize(initialize_params, completable);
    server.endpoint.request_shutdown();

    // Assert there is one edge between two nodes
    assert_eq!(server.graph.borrow().graph.edge_count(), 1);

    let edge = server.graph.borrow().graph.edge_indices().next().unwrap();
    let (node1, node2) = server.graph.borrow().graph.edge_endpoints(edge).unwrap();

    // Assert the values of the two nodes in the tree
    assert_eq!(
        server.graph.borrow().graph[node1],
        //format!("{:?}/{}/{}", tmp_path, "shaders", "final.fsh")
        tmp_path.join("shaders").join("final.fsh").to_str().unwrap().to_string()
    );
    assert_eq!(
        server.graph.borrow().graph[node2],
        //format!("{:?}/{}/{}", tmp_path, "shaders", "common.glsl")
        tmp_path.join("shaders").join("common.glsl").to_str().unwrap().to_string()
    );

    assert_eq!(server.graph.borrow().graph.edge_weight(edge).unwrap().line, 2);
}

#[allow(deprecated)]
#[test]
#[logging_macro::log_scope]
fn test_05_initialize() {
    let mut server = new_temp_server(None);

    let (_tmp_dir, tmp_path) = copy_to_tmp_dir("./testdata/05");

    let initialize_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::from_directory_path(tmp_path.clone()).unwrap()),
        client_info: None,
        initialization_options: None,
        capabilities: ClientCapabilities {
            workspace: None,
            text_document: None,
            experimental: None,
            window: None,
            general: Option::None,
        },
        trace: None,
        workspace_folders: None,
        locale: Option::None,
    };

    let on_response = |resp: Option<Response>| {
        assert!(resp.is_some());
        let respu = resp.unwrap();
        match respu.result_or_error {
            ResponseResult::Result(_) => {}
            ResponseResult::Error(e) => {
                panic!("expected ResponseResult::Result(..), got {:?}", e)
            }
        }
    };

    let completable = MethodCompletable::new(ResponseCompletable::new(Some(Id::Number(1)), Box::new(on_response)));
    server.initialize(initialize_params, completable);
    server.endpoint.request_shutdown();

    // Assert there is one edge between two nodes
    assert_eq!(server.graph.borrow().graph.edge_count(), 3);

    assert_eq!(server.graph.borrow().graph.node_count(), 4);

    let pairs: HashSet<(PathBuf, PathBuf)> = vec![
        (
            tmp_path.join("shaders").join("final.fsh").to_str().unwrap().to_string().into(),
            tmp_path.join("shaders").join("common.glsl").to_str().unwrap().to_string().into(),
        ),
        (
            tmp_path.join("shaders").join("final.fsh").to_str().unwrap().to_string().into(),
            tmp_path
                .join("shaders")
                .join("test")
                .join("banana.glsl")
                .to_str()
                .unwrap()
                .to_string()
                .into(),
        ),
        (
            tmp_path
                .join("shaders")
                .join("test")
                .join("banana.glsl")
                .to_str()
                .unwrap()
                .to_string()
                .into(),
            tmp_path
                .join("shaders")
                .join("test")
                .join("burger.glsl")
                .to_str()
                .unwrap()
                .to_string()
                .into(),
        ),
    ]
    .into_iter()
    .collect();

    for edge in server.graph.borrow().graph.edge_indices() {
        let endpoints = server.graph.borrow().graph.edge_endpoints(edge).unwrap();
        let first = server.graph.borrow().get_node(endpoints.0);
        let second = server.graph.borrow().get_node(endpoints.1);
        let contains = pairs.contains(&(first.clone(), second.clone()));
        assert!(contains, "doesn't contain ({:?}, {:?})", first, second);
    }
}

#[allow(deprecated)]
#[test]
#[logging_macro::log_scope]
fn test07_rewrited_file_system() {
    let mut server = new_temp_server(None);

    let (_tmp_dir, tmp_path) = copy_to_tmp_dir("./testdata/05");

    let initialize_params = InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: Some(Url::from_directory_path(tmp_path.clone()).unwrap()),
        client_info: None,
        initialization_options: None,
        capabilities: ClientCapabilities {
            workspace: None,
            text_document: None,
            experimental: None,
            window: None,
            general: Option::None,
        },
        trace: None,
        workspace_folders: None,
        locale: Option::None,
    };

    let on_response = |resp: Option<Response>| {
        assert!(resp.is_some());
        let respu = resp.unwrap();
        match respu.result_or_error {
            ResponseResult::Result(_) => {}
            ResponseResult::Error(e) => {
                panic!("expected ResponseResult::Result(..), got {:?}", e)
            }
        }
    };

    let completable = MethodCompletable::new(ResponseCompletable::new(Some(Id::Number(1)), Box::new(on_response)));
    server.initialize(initialize_params, completable);
    server.endpoint.request_shutdown();
    
    info!("detected {} shader files", server.shader_files.len());

    // let mut shader_files: String = String::from("shader files are");
    for file in &server.shader_files {

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = file.1.merge_shader_file(&server.include_files, &mut file_list);
        info!("{}", shader_content);

        info!("{}", file.1.file_type());

        let compile_log = match server.opengl_context.clone().validate_shader(file.1.file_type(), &shader_content) {
            Some(log) => log,
            None => "".to_string()
        };

        info!("{}", compile_log);

        // shader_files += "\n\t";
        // shader_files += &String::from(file.0.to_str().unwrap());
        // shader_files += "\n\t\tincludes :";
        // let include_files = file.1.including_files();
        // let mut index = 0;
        // for ele in include_files {
        //     shader_files += "\n\t\t\t";
        //     let line = ele.0;
        //     let include_file = &ele.1;
        //     shader_files += "index: ";
        //     shader_files += &index.to_string();
        //     shader_files += "\t line: ";
        //     shader_files += &line.to_string();
        //     shader_files += "\t path: ";
        //     shader_files += &String::from(include_file.to_str().unwrap());
        //     index += 1;
        // }
    }
    // info!("{}", &shader_files);

    // info!("detected {} include files", server.include_files.len());

    // let mut include_files: String = String::from("include files are");
    // for file in &server.include_files {
    //     include_files += "\n\t";
    //     include_files += &String::from(file.0.to_str().unwrap());
    //     include_files += "\n\t\trelated to :";
    //     let parents = file.1.included_shaders().clone();
    //     for ele in parents {
    //         include_files += "\n\t\t\t";
    //         include_files += &String::from(ele.to_str().unwrap());
    //     }
    //     include_files += "\n\t\tsub files :";
    //     let mut index = 0;
    //     let sub_files = file.1.including_files();
    //     for ele in sub_files {
    //         include_files += "\n\t\t\t";
    //         let line = ele.0;
    //         let include_file = &ele.1;
    //         include_files += "index: ";
    //         include_files += &index.to_string();
    //         include_files += "\t line: ";
    //         include_files += &line.to_string();
    //         include_files += "\t path: ";
    //         include_files += &String::from(include_file.to_str().unwrap());
    //         index += 1;
    //     }
    // }
    // info!("{}", &include_files);

    // for shader in server.shader_files {
    //     info!("{}", shader.0.to_str().unwrap());
    // }

}
