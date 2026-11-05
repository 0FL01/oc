use super::*;
use std::fs;
use std::os::unix::fs::symlink;

fn setup() -> (tempfile::TempDir, Files) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let files = Files::new(&root, &temp.path().join("data")).unwrap();
    (temp, files)
}
fn read(files: &Files, path: &str, offset: u64, limit: usize) -> Result<Content, FileToolError> {
    files.read_admitted(path, offset, limit, |_| true, &AtomicBool::new(false))
}

#[test]
fn tool16_text_pages_default_full_tail_and_legacy_compatibility() {
    let (_temp, files) = setup();
    let body = (1..=2005)
        .map(|i| format!("fixture_{i}\r\n"))
        .collect::<String>();
    fs::write(files.root.join("large"), body).unwrap();
    let first = files.read("large", 1, 2000).unwrap();
    assert_eq!(first.lines.len(), 2000);
    assert_eq!(first.lines.last().unwrap(), "fixture_2000");
    assert_eq!(first.next_offset, Some(2001));
    let tail = files.read("large", 2001, 2000).unwrap();
    assert_eq!(tail.lines.len(), 5);
    assert!(!tail.truncated);
    assert!(read(&files, "large", 2006, 2).is_err());
    fs::write(files.root.join("tail-binary"), b"safe\n\0CANARY").unwrap();
    assert_eq!(files.read("tail-binary", 1, 1).unwrap().lines, ["safe"]);
    assert!(matches!(
        read(&files, "tail-binary", 2, 1),
        Err(FileToolError::Binary)
    ));
    assert!(
        matches!(
            read(&files, "tail-binary", 3, 1),
            Err(FileToolError::Binary)
        ),
        "skipped binary lines are still consumed prefix"
    );
    fs::write(files.root.join("long-line"), "x".repeat(READ_BYTES_CAP + 1)).unwrap();
    let long = files.read("long-line", 1, 1).unwrap();
    assert!(long.truncated && long.lines[0].len() < READ_BYTES_CAP);
    assert!(long.lines[0].contains("Line truncated"));
    fs::write(
        files.root.join("oversize"),
        vec![b'x'; GREP_FILE_BYTES_CAP as usize + 1],
    )
    .unwrap();
    assert!(files.read("oversize", 1, 1).unwrap().truncated);
}

#[test]
fn w2_large_text_streams_pages_offsets_and_utf8_without_whole_file_admission() {
    let (_temp, files) = setup();
    let mut body = "first\n".to_owned();
    body.push_str(&"padding\n".repeat(150_000));
    body.push_str("distant 界\n");
    fs::write(files.root.join("large"), &body).unwrap();
    assert!(body.len() > GREP_FILE_BYTES_CAP as usize);
    let first = files.read("large", 1, 1).unwrap();
    assert_eq!(first.lines, ["first"]);
    assert_eq!(first.next_offset, Some(2));
    let tail = files.read("large", 150_002, 1).unwrap();
    assert_eq!(tail.lines, ["distant 界"]);
    assert_eq!(tail.next_offset, None);
    fs::write(
        files.root.join("large"),
        format!("{}界\nend\n", "x".repeat(2 * 1024 * 1024 - 1)),
    )
    .unwrap();
    let long = files.read("large", 1, 1).unwrap();
    assert!(long.truncated && long.lines[0].len() < READ_BYTES_CAP);
    assert_eq!(long.next_offset, Some(2));
    assert_eq!(files.read("large", 2, 1).unwrap().lines, ["end"]);
    fs::write(
        files.root.join("boundary"),
        format!("{}界\nnext\n", "x".repeat(8191)),
    )
    .unwrap();
    assert_eq!(
        files.read("boundary", 1, 1).unwrap().lines[0],
        format!("{}界", "x".repeat(8191))
    );
}

#[test]
fn w2_explicit_symlink_read_uses_lexical_permission_and_resolved_data_exclusion() {
    let (temp, files) = setup();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file"), "external\n").unwrap();
    symlink(&outside, files.root.join("link")).unwrap();
    let lexical = files.root.join("link/file");
    let Content::Text(page) = files
        .read_admitted(
            "link/file",
            1,
            1,
            |path| path == lexical,
            &AtomicBool::new(false),
        )
        .unwrap()
    else {
        panic!("text")
    };
    assert_eq!(page.lines, ["external"]);
    fs::create_dir(&files.data_root).unwrap();
    fs::write(files.data_root.join("private"), "secret\n").unwrap();
    symlink(&files.data_root, files.root.join("data-alias")).unwrap();
    assert!(matches!(
        read(&files, "data-alias/private", 1, 1),
        Err(FileToolError::OwnDataRoot)
    ));
}

#[test]
fn tool16_directory_sorted_paged_names_and_policy_no_implicit_symlink_fetch() {
    let (_temp, files) = setup();
    fs::create_dir(files.root.join("z-dir")).unwrap();
    fs::write(files.root.join("b"), "b").unwrap();
    fs::write(files.root.join("a"), "a").unwrap();
    fs::write(files.root.join("denied"), "PROTECTED_CANARY").unwrap();
    symlink("denied", files.root.join("link")).unwrap();
    let policy = |p: &Path| p.file_name().is_none_or(|n| n != "denied");
    let Content::Directory(first) = files
        .read_admitted(".", 1, 2, policy, &AtomicBool::new(false))
        .unwrap()
    else {
        panic!("directory")
    };
    assert_eq!(first.lines, ["z-dir/", "a"]);
    assert_eq!(first.next_offset, Some(3));
    let Content::Directory(tail) = files
        .read_admitted(".", 3, 2, policy, &AtomicBool::new(false))
        .unwrap()
    else {
        panic!("directory")
    };
    assert_eq!(tail.lines, ["b", "link"]);
    assert!(!tail.truncated);
    fs::write(files.root.join("bad\nname"), "x").unwrap();
    assert!(read(&files, ".", 1, 10).is_err());
}

#[test]
fn tool16_descriptor_root_pin_ancestor_swap_fifo_data_root_and_cancel() {
    let (temp, files) = setup();
    fs::write(files.root.join("safe"), "original\n").unwrap();
    fs::create_dir(temp.path().join("outside")).unwrap();
    fs::write(temp.path().join("outside/secret"), "PROTECTED_CANARY").unwrap();
    symlink(temp.path().join("outside"), files.root.join("ancestor")).unwrap();
    assert_eq!(
        files.read("ancestor/secret", 1, 1).unwrap().lines,
        ["PROTECTED_CANARY"]
    );
    let Content::Text(page) = files
        .read_admitted(
            "safe",
            1,
            1,
            |_| {
                fs::rename(files.root.join("safe"), files.root.join("held")).unwrap();
                symlink(temp.path().join("outside/secret"), files.root.join("safe")).unwrap();
                true
            },
            &AtomicBool::new(false),
        )
        .unwrap()
    else {
        panic!("text")
    };
    assert_eq!(page.lines, ["PROTECTED_CANARY"]);
    fs::remove_file(files.root.join("safe")).unwrap();
    fs::rename(files.root.join("held"), files.root.join("safe")).unwrap();
    let fifo = CString::new(files.root.join("fifo").as_os_str().as_bytes()).unwrap();
    // SAFETY: mkfifo receives a live NUL-terminated owned fixture path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(read(&files, "fifo", 1, 1).is_err());
    fs::create_dir(files.root.join("data")).unwrap();
    fs::write(files.root.join("data/secret"), "PROTECTED_CANARY").unwrap();
    let restricted = Files::new(&files.root, &files.root.join("data")).unwrap();
    assert!(matches!(
        read(&restricted, "data/secret", 1, 1),
        Err(FileToolError::OwnDataRoot)
    ));
    assert!(matches!(
        files.read_admitted(
            "safe",
            1,
            1,
            |_| panic!("cancel must precede policy/IO"),
            &AtomicBool::new(true)
        ),
        Err(FileToolError::Cancelled)
    ));
    let original = temp.path().join("original");
    fs::rename(&files.root, &original).unwrap();
    fs::create_dir(&files.root).unwrap();
    fs::write(files.root.join("safe"), "replacement\n").unwrap();
    assert_eq!(files.read("safe", 1, 1).unwrap().lines, ["original"]);
}

#[test]
fn tool16_real_decoding_rejects_signature_only_pdf_and_invalid_image() {
    let (_temp, files) = setup();
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
    let mut encoded = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let bytes = encoded.into_inner();
    fs::write(files.root.join("real.png"), &bytes).unwrap();
    let Content::Image {
        bytes: actual,
        mime,
        ..
    } = read(&files, "real.png", 1, 2000).unwrap()
    else {
        panic!("image")
    };
    assert_eq!(actual, bytes);
    assert_eq!(mime, "image/png");
    fs::write(files.root.join("fake.png"), b"\x89PNG\r\n\x1a\n\0FAKE").unwrap();
    assert!(read(&files, "fake.png", 1, 2000).is_err());
    fs::write(files.root.join("truncated.png"), &bytes[..bytes.len() / 2]).unwrap();
    assert!(read(&files, "truncated.png", 1, 2000).is_err());
    fs::write(files.root.join("no-end.png"), &bytes[..bytes.len() - 12]).unwrap();
    assert!(read(&files, "no-end.png", 1, 2000).is_err());
    fs::write(files.root.join("bad.png"), "not an image").unwrap();
    assert!(read(&files, "bad.png", 1, 2000).is_err());
    fs::write(files.root.join("document"), b"%PDF-1.7 text").unwrap();
    assert!(read(&files, "document", 1, 2000).is_err());
    assert!(
        files.read("real.png", 1, 1).is_err(),
        "String-only legacy API cannot claim image success"
    );
    let cap = crate::attachments::ATTACHMENT_CAP_BYTES as u64;
    fs::File::create(files.root.join("oversize.png"))
        .unwrap()
        .set_len(cap + 1)
        .unwrap();
    assert!(matches!(read(&files, "oversize.png", 1, 1),
        Err(FileToolError::BudgetExhausted(facts)) if facts.budget == "file_bytes" && facts.limit == cap && facts.observed == cap + 1));
}

#[test]
fn tool16_all_animation_frames_must_decode_under_existing_pixel_budget() {
    let (_temp, files) = setup();
    let frame = || {
        image::Frame::new(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([1, 2, 3, 255]),
        ))
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        encoder.encode_frame(frame()).unwrap();
        encoder.encode_frame(frame()).unwrap();
    }
    fs::write(files.root.join("animated.gif"), &bytes).unwrap();
    assert!(matches!(
        read(&files, "animated.gif", 1, 1),
        Ok(Content::Image { .. })
    ));
    bytes.truncate(bytes.len() - 8);
    bytes.push(b';'); // a complete first frame must not hide an invalid second frame
    fs::write(files.root.join("bad-animation.gif"), bytes).unwrap();
    assert!(read(&files, "bad-animation.gif", 1, 1).is_err());
    let pixels = image::RgbaImage::from_pixel(3072, 3072, image::Rgba([0, 0, 0, 255]));
    let mut encoded = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    assert!(encoded.get_ref().len() < GREP_FILE_BYTES_CAP as usize);
    fs::write(files.root.join("pixel-budget.png"), encoded.into_inner()).unwrap();
    assert!(read(&files, "pixel-budget.png", 1, 1).is_err());
}
