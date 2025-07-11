#[cfg(test)]
pub mod helpers {
    use ropey::Rope;
    use serde_json::to_value;

    use std::{
        collections::{BTreeSet, HashMap, HashSet},
        sync::{Arc, LazyLock},
    };
    use tower::{Service, ServiceExt};
    use tree_sitter::Parser;

    use dashmap::DashMap;
    use tower_lsp::{
        LspService,
        jsonrpc::{Request, Response},
        lsp_types::{
            ClientCapabilities, InitializeParams, Position, Range, TextDocumentContentChangeEvent,
            TextEdit, Url, request::Initialize,
        },
    };

    use crate::{
        Backend, DocumentData, LanguageData, Options, QUERY_LANGUAGE, SymbolInfo,
        util::get_language_name,
    };

    pub static TEST_URI: LazyLock<Url> =
        LazyLock::new(|| Url::parse("file:///tmp/queries/js/test.scm").unwrap());
    pub static TEST_URI_2: LazyLock<Url> =
        LazyLock::new(|| Url::parse("file:///tmp/queries/css/test.scm").unwrap());
    pub const SIMPLE_FILE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/queries/example_test_files/simple.scm"
    ));
    pub const COMPLEX_FILE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/queries/example_test_files/complex.scm"
    ));

    /// Always test with id of 1 for simplicity
    const ID: i64 = 1;

    /// A tuple holding the document's URI, source text, symbols, fields, supertypes, and valid
    /// captures
    pub type Document<'a> = (
        Url,
        &'a str,
        Vec<SymbolInfo>,
        Vec<&'a str>,
        Vec<&'a str>,
        Vec<(u32, u32, Option<Url>)>,
    );

    /// Initialize a test server, populating it with fake documents denoted by (uri, text, symbols, fields) tuples.
    pub async fn initialize_server(
        documents: &[Document<'_>],
        options: &Options,
    ) -> LspService<Backend> {
        let mut parser = Parser::new();
        parser
            .set_language(&QUERY_LANGUAGE)
            .expect("Error loading Query grammar");
        let options_value = serde_json::to_value(options).unwrap();
        let options = &serde_json::from_value::<Options>(options_value.clone()).unwrap();
        let arced_options = Arc::new(tokio::sync::RwLock::new(options.clone()));
        let (mut service, _socket) = LspService::build(|client| Backend {
            _client: client,
            document_map: DashMap::from_iter(documents.iter().map(
                |(uri, source, _, _, _, imported_uris)| {
                    (
                        uri.clone(),
                        DocumentData {
                            rope: Rope::from(*source),
                            tree: parser.parse(*source, None).unwrap(),
                            version: 0,
                            language_name: get_language_name(uri, options),
                            imported_uris: imported_uris.clone(),
                        },
                    )
                },
            )),
            language_map: DashMap::from_iter(documents.iter().map(
                |(uri, _, symbols, fields, supertypes, _)| {
                    let language_name = get_language_name(uri, options).unwrap();
                    (
                        language_name.clone(),
                        LanguageData {
                            name: language_name,
                            language: None,
                            symbols_set: HashSet::from_iter(symbols.clone()),
                            symbols_vec: symbols.clone(),
                            fields_set: HashSet::from_iter(fields.iter().map(ToString::to_string)),
                            fields_vec: fields.clone().iter().map(ToString::to_string).collect(),
                            supertype_map: HashMap::from_iter(supertypes.iter().map(|supertype| {
                                (
                                    SymbolInfo {
                                        named: true,
                                        label: String::from(*supertype),
                                    },
                                    BTreeSet::from([
                                        SymbolInfo {
                                            named: true,
                                            label: String::from("test"),
                                        },
                                        SymbolInfo {
                                            named: true,
                                            label: String::from("test2"),
                                        },
                                    ]),
                                )
                            })),
                        }
                        .into(),
                    )
                },
            )),
            workspace_uris: Default::default(),
            options: arced_options,
        })
        .finish();

        service
            .ready()
            .await
            .unwrap()
            .call(lsp_request_to_jsonrpc_request::<Initialize>(
                InitializeParams {
                    capabilities: ClientCapabilities::default(),
                    root_uri: Some(Url::parse("file:///tmp/").unwrap()),
                    initialization_options: Some(options_value),
                    ..Default::default()
                },
            ))
            .await
            .unwrap()
            .unwrap();

        service
    }

    // An equivalent function is provided but it is private
    pub fn lsp_request_to_jsonrpc_request<R>(params: R::Params) -> Request
    where
        R: tower_lsp::lsp_types::request::Request,
    {
        Request::build(R::METHOD)
            .id(ID)
            .params(to_value(params).unwrap())
            .finish()
    }

    pub fn lsp_notification_to_jsonrpc_request<R>(params: R::Params) -> Request
    where
        R: tower_lsp::lsp_types::notification::Notification,
    {
        Request::build(R::METHOD)
            .params(to_value(params).unwrap())
            .finish()
    }

    pub fn lsp_response_to_jsonrpc_response<R>(params: R::Result) -> Response
    where
        R: tower_lsp::lsp_types::request::Request,
    {
        Response::from_ok(ID.into(), to_value(params).unwrap())
    }

    pub fn jsonrpc_response_to_lsp_value<R>(response: Response) -> R::Result
    where
        R: tower_lsp::lsp_types::request::Request,
    {
        serde_json::from_value::<R::Result>(response.result().unwrap().clone()).unwrap()
    }

    #[derive(Debug, Clone)]
    pub struct TestEdit {
        pub text: String,
        pub range: Range,
    }

    impl TestEdit {
        pub fn new(text: &str, start: (u32, u32), end: (u32, u32)) -> Self {
            Self {
                text: text.to_string(),
                range: Range {
                    start: Position {
                        line: start.0,
                        character: start.1,
                    },
                    end: Position {
                        line: end.0,
                        character: end.1,
                    },
                },
            }
        }
    }

    impl From<&TestEdit> for TextDocumentContentChangeEvent {
        fn from(val: &TestEdit) -> Self {
            Self {
                range: Some(val.range),
                range_length: None,
                text: val.text.clone(),
            }
        }
    }

    impl From<&TestEdit> for TextEdit {
        fn from(val: &TestEdit) -> Self {
            Self {
                range: val.range,
                new_text: val.text.clone(),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::{BTreeMap, HashMap},
        ops::Deref,
    };
    use ts_query_ls::Options;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use crate::{
        SymbolInfo,
        test_helpers::helpers::{
            COMPLEX_FILE, SIMPLE_FILE, TEST_URI, TEST_URI_2, initialize_server,
        },
        util::get_language_name,
    };

    use super::helpers::Document;

    #[rstest]
    #[case(&[], &Default::default())]
    #[case(&[(
        TEST_URI.clone(),
        SIMPLE_FILE,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ), (
        TEST_URI_2.clone(),
        COMPLEX_FILE,
        vec![
            SymbolInfo { named: true, label: String::from("identifier") },
            SymbolInfo { named: false, label: String::from(";") }
        ],
        vec![
            "operator",
            "content",
        ],
        vec!["type"],
        vec![(0, 2, None)],
    )],
        &Options {
            valid_captures: HashMap::from([(String::from("test"), BTreeMap::from([(String::from("variable"), String::from("A common variable"))]))]),
            ..Default::default()
        }
    )]
    #[tokio::test(flavor = "current_thread")]
    async fn initialize_server_helper(
        #[case] documents: &[Document<'_>],
        #[case] options: &Options,
    ) {
        // Act
        let service = initialize_server(documents, options).await;

        // Assert
        let backend = service.inner();
        // Serialize and re-serialize to populate the required fields that are added at
        // deserialization time (default language retrieval regexes, `not-` predicates)
        let options =
            &serde_json::from_value::<Options>(serde_json::to_value(options).unwrap()).unwrap();

        let actual_options = backend.options.read().await;
        assert_eq!(actual_options.deref(), options);
        assert_eq!(backend.document_map.len(), documents.len());
        for (uri, source, symbols, fields, supertypes, imported_urls) in documents {
            let doc = backend.document_map.get(uri).unwrap();
            assert_eq!(doc.rope.to_string(), (*source).to_string());
            assert_eq!(
                doc.tree
                    .root_node()
                    .utf8_text((*source).to_string().as_bytes())
                    .unwrap(),
                (*source).to_string()
            );
            let language_name = get_language_name(uri, options).unwrap();
            let language_data = backend.language_map.get(&language_name).unwrap();
            assert!(language_data.symbols_vec.len() == symbols.len());
            assert!(language_data.symbols_set.len() == symbols.len());
            for symbol in symbols {
                assert!(language_data.symbols_vec.contains(symbol));
                assert!(language_data.symbols_set.contains(symbol));
            }
            assert!(language_data.fields_vec.len() == fields.len());
            assert!(language_data.fields_set.len() == fields.len());
            for field in fields {
                assert!(language_data.fields_vec.contains(&field.to_string()));
                assert!(language_data.fields_set.contains(*field));
            }
            for supertype in supertypes {
                assert!(language_data.supertype_map.contains_key(&SymbolInfo {
                    named: true,
                    label: String::from(*supertype)
                }))
            }
            assert_eq!(imported_urls, &doc.imported_uris);
        }
    }
}
