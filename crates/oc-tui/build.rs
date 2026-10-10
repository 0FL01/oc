//! Compile only repository-vendored, pinned grammar sources. No network/JS/WASM
//! is needed by this build or by the resulting application.

use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
};

fn verified(assets: &Path, file: &str, expected: &str) -> Vec<u8> {
    assert!(
        Path::new(file).file_name().is_some_and(|name| name == file),
        "asset basename"
    );
    let bytes = fs::read(assets.join(file)).expect("vendored syntax asset");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        expected,
        "changed syntax asset: {file}"
    );
    bytes
}

fn main() {
    let assets = PathBuf::from("assets/syntax");
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(assets.join("manifest.json")).expect("vendored syntax manifest"),
    )
    .expect("valid vendored syntax manifest");
    assert_eq!(manifest["version"], 1, "supported syntax manifest");
    assert_eq!(
        manifest["grammars"]
            .as_array()
            .expect("grammar inventory")
            .len(),
        39,
        "complete frozen native inventory"
    );
    for license in manifest["query_licenses"]
        .as_array()
        .expect("query license inventory")
    {
        verified(
            &assets,
            license["file"].as_str().expect("license file"),
            license["sha256"].as_str().expect("license digest"),
        );
    }
    let reference: serde_json::Value = serde_json::from_slice(
        &fs::read(assets.join("fixtures.reference.json")).expect("actual reference corpus"),
    )
    .expect("reference corpus JSON");
    verified(
        &assets,
        "manifest.json",
        reference["manifest_sha256"]
            .as_str()
            .expect("reference manifest digest"),
    );
    verified(
        &assets,
        "fixtures.json",
        reference["fixtures_sha256"]
            .as_str()
            .expect("reference fixture digest"),
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    println!("cargo:rerun-if-changed=assets/syntax");
    let mut bindings = String::from("// Generated native bindings for the vetted manifest.\n");
    let mut entries = format!(
        "static GRAMMARS: [Grammar; {}] = [\n",
        manifest["grammars"]
            .as_array()
            .expect("grammar inventory")
            .len()
    );
    for item in manifest["grammars"].as_array().expect("grammar inventory") {
        let name = item["name"].as_str().expect("grammar name");
        let symbol = item["symbol"].as_str().expect("native entry");
        let source = output.join(name);
        fs::create_dir_all(&source).expect("grammar build directory");
        let compressed = verified(
            &assets,
            item["archive"].as_str().expect("source archive"),
            item["archive_sha256"]
                .as_str()
                .expect("source archive digest"),
        );
        for query in item["queries"]
            .as_object()
            .expect("query inventory")
            .values()
        {
            verified(
                &assets,
                query["file"].as_str().expect("query file"),
                query["sha256"].as_str().expect("query digest"),
            );
            for part in query["parts"].as_array().expect("query source parts") {
                verified(
                    &assets,
                    part["file"].as_str().expect("query part file"),
                    part["sha256"].as_str().expect("query part digest"),
                );
            }
        }
        tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(compressed)))
            .unpack(&source)
            .expect("vendored native grammar sources");
        let parser = source.join(item["parser"].as_str().expect("parser source"));
        let include = parser.parent().expect("parser source parent");
        let mut c = cc::Build::new();
        c.std("c11").include(include).include(&source).file(&parser);
        let mut cpp = cc::Build::new();
        cpp.cpp(true).std("c++17").include(include).include(&source);
        let mut has_cpp = false;
        for scanner in item["scanners"].as_array().expect("scanner inventory") {
            let scanner = source.join(scanner.as_str().expect("scanner source"));
            if scanner.extension().is_some_and(|ext| ext == "c") {
                c.file(scanner);
            } else {
                has_cpp = true;
                cpp.file(scanner);
            }
        }
        // Generated third-party grammars deliberately contain unused states and
        // parameters; their build is not the project's handwritten lint surface.
        c.warnings(false).compile(&format!("oc_syntax_{name}"));
        if has_cpp {
            cpp.warnings(false)
                .compile(&format!("oc_syntax_{name}_scanner"));
        }
        bindings.push_str(&format!(
            "unsafe extern \"C\" {{ fn {symbol}() -> *const (); }}\n\
             fn language_{name}() -> tree_sitter::Language {{\n\
             // SAFETY: this statically linked, vetted entry returns the immutable\n\
             // TSLanguage generated for the manifest's exact source revision.\n\
             let entry = unsafe {{ tree_sitter_language::LanguageFn::from_raw({symbol}) }};\n\
             tree_sitter::Language::from(entry)\n}}\n"
        ));
        let highlights = item["queries"]["highlights"]["file"]
            .as_str()
            .expect("required highlight query");
        let injections = item["queries"]["injections"]["file"].as_str();
        let query = |file: &str| {
            format!(
                "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/syntax/{file}\"))"
            )
        };
        let pairs = |value: &serde_json::Value| {
            let mut result = String::from("&[");
            if let Some(entries) = value.as_object() {
                for (key, value) in entries {
                    result.push_str(&format!(
                        "({key:?}, {:?}),",
                        value.as_str().expect("mapping target")
                    ));
                }
            }
            result.push(']');
            result
        };
        entries.push_str(&format!(
            "Grammar {{ name: {name:?}, aliases: &{}, language: language_{name}, highlights: {}, injections: {}, injection_nodes: {}, injection_info: {}, compiled: OnceLock::new() }},\n",
            item["aliases"],
            query(highlights),
            injections.map_or_else(|| "\"\"".to_owned(), query),
            pairs(&item["injection_mapping"]["nodeTypes"]),
            pairs(&item["injection_mapping"]["infoStringMap"]),
        ));
    }
    entries.push_str("];\n");
    bindings.push_str(&entries);
    bindings.push_str("static PATH_TYPES: &[(&str, &str)] = &[\n");
    for (extension, language) in manifest["filetypes"]
        .as_object()
        .expect("donor extension inventory")
    {
        bindings.push_str(&format!(
            "({extension:?}, {:?}),\n",
            language.as_str().expect("filetype name")
        ));
    }
    bindings.push_str("];\n");
    for (name, key) in [
        ("INFO_EXTENSIONS", "extensions"),
        ("INFO_BASENAMES", "basenames"),
    ] {
        bindings.push_str(&format!("static {name}: &[(&str, &str)] = &[\n"));
        for (name, value) in manifest["info_resolver"][key]
            .as_object()
            .expect("actual SDK resolver")
        {
            bindings.push_str(&format!(
                "({name:?}, {:?}),\n",
                value.as_str().expect("filetype")
            ));
        }
        bindings.push_str("];\n");
    }
    fs::write(output.join("syntax_languages.rs"), bindings).expect("native syntax bindings");
}
