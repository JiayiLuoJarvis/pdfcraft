//! Overlay isolation: wrap original `/Contents` and reset graphics state.

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]
#![expect(
    clippy::unwrap_used,
    reason = "integration test: a panic is the failure report"
)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use pdfcraft_forms::{
    AbsoluteLine, ErrorKind, FieldFill, FieldRule, Fill, FillRequest, FlattenReason, Font, Form,
};

const PAGE: i16 = 200;
const TEXT_RECT: [i16; 4] = [20, 150, 180, 180];
const CHECK_RECT: [i16; 4] = [20, 80, 50, 110];
const RULE_OFFSET: f32 = 4.0;
const LEAKY: &[u8] = b"/GSz gs 1 G 1 g 3 Tr 0.5 Tc 120 Tz 2 Ts [3 2] 0 d 2 J\n";
const CM_LEAK: &[u8] = b"2 0 0 2 0 0 cm\n";
const FILL_WORD: &str = "Ada";

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum ContentsShape {
    Stream,
    DirectArray,
    IndirectArray,
    Missing,
}

fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

fn rect_array(rect: [i16; 4]) -> Object {
    Object::Array(
        rect.into_iter()
            .map(|n| Object::Integer(i64::from(n)))
            .collect(),
    )
}

fn mediabox() -> Object {
    Object::Array(vec![
        Object::Integer(0),
        Object::Integer(0),
        Object::Integer(i64::from(PAGE)),
        Object::Integer(i64::from(PAGE)),
    ])
}

fn dummy_gs() -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("ca", 0);
    dict
}

fn gsz_dict() -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "ExtGState");
    dict.set("ca", 0);
    dict.set("CA", 0);
    dict
}

fn extgstate_with(names: &[&str]) -> Dictionary {
    let mut ext = Dictionary::new();
    ext.set("GSz", gsz_dict());
    for extra in names {
        ext.set(*extra, dummy_gs());
    }
    ext
}

fn resources_with_ext(ext: Dictionary) -> Dictionary {
    let mut resources = Dictionary::new();
    resources.set("ExtGState", ext);
    resources
}

fn widget(doc: &mut Document, field: &str, ft: &str, rect: [i16; 4]) -> ObjectId {
    doc.add_object(Dictionary::from_iter([
        ("Type", name("Annot")),
        ("Subtype", name("Widget")),
        ("T", Object::string_literal(field)),
        ("FT", name(ft)),
        ("Rect", rect_array(rect)),
    ]))
}

fn text_and_check(doc: &mut Document) -> Vec<Object> {
    vec![
        Object::Reference(widget(doc, "title", "Tx", TEXT_RECT)),
        Object::Reference(widget(doc, "agree", "Btn", CHECK_RECT)),
    ]
}

fn finish(mut doc: Document, page_tree: ObjectId, pages: &[ObjectId]) -> Vec<u8> {
    doc.set_object(
        page_tree,
        Dictionary::from_iter([
            ("Type", name("Pages")),
            (
                "Count",
                Object::Integer(i64::try_from(pages.len()).unwrap()),
            ),
            (
                "Kids",
                Object::Array(pages.iter().copied().map(Object::Reference).collect()),
            ),
        ]),
    );
    let catalog = doc.add_object(Dictionary::from_iter([
        ("Type", name("Catalog")),
        ("Pages", Object::Reference(page_tree)),
    ]));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn set_page(
    doc: &mut Document,
    page: ObjectId,
    page_tree: ObjectId,
    resources: Dictionary,
    contents: Option<Object>,
    annots: Vec<Object>,
) {
    let mut dict = Dictionary::from_iter([
        ("Type", name("Page")),
        ("Parent", Object::Reference(page_tree)),
        ("MediaBox", mediabox()),
        ("Resources", Object::Dictionary(resources)),
        ("Annots", Object::Array(annots)),
    ]);
    if let Some(contents) = contents {
        dict.set("Contents", contents);
    }
    doc.set_object(page, dict);
}

fn stream_obj(doc: &mut Document, bytes: &[u8]) -> ObjectId {
    doc.add_object(Object::Stream(
        Stream::new(Dictionary::new(), bytes.to_vec()).with_compression(false),
    ))
}

fn contents_object(doc: &mut Document, shape: ContentsShape) -> (Option<Object>, Vec<Object>) {
    match shape {
        ContentsShape::Missing => (None, Vec::new()),
        ContentsShape::Stream => {
            let id = stream_obj(doc, LEAKY);
            (Some(Object::Reference(id)), vec![Object::Reference(id)])
        }
        ContentsShape::DirectArray => {
            let a = stream_obj(doc, LEAKY);
            let b = stream_obj(doc, b"\n");
            let original = vec![Object::Reference(a), Object::Reference(b)];
            (Some(Object::Array(original.clone())), original)
        }
        ContentsShape::IndirectArray => {
            let a = stream_obj(doc, LEAKY);
            let b = stream_obj(doc, b"\n");
            let original = vec![Object::Reference(a), Object::Reference(b)];
            let array_id = doc.add_object(Object::Array(original.clone()));
            (Some(Object::Reference(array_id)), original)
        }
    }
}

fn leaky_page_pdf(shape: ContentsShape) -> (Vec<u8>, Vec<Object>) {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    let (contents, original) = contents_object(&mut doc, shape);
    set_page(
        &mut doc,
        page,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        contents,
        annots,
    );
    (finish(doc, page_tree, &[page]), original)
}

fn marks_request() -> FillRequest {
    FillRequest {
        rules: BTreeMap::from([("title".to_owned(), FieldRule::new(RULE_OFFSET))]),
        lines: vec![AbsoluteLine::new(30.0, 40.0, 170.0, 40.0)],
        ..FillRequest::new(BTreeMap::from([(
            "agree".to_owned(),
            FieldFill::check(true),
        )]))
    }
}

fn text_request() -> FillRequest {
    FillRequest::new(BTreeMap::from([(
        "title".to_owned(),
        FieldFill::text(FILL_WORD),
    )]))
}

fn draw(pdf: &[u8], request: FillRequest, font: Option<&Font>) -> pdfcraft_forms::Result<Vec<u8>> {
    let form = Form::open(pdf)?;
    Fill::new(&form, request, font)?.to_pdf()
}

fn load(pdf: &[u8]) -> Document {
    Document::load_mem(pdf).unwrap()
}

fn page_id(doc: &Document, index: usize) -> ObjectId {
    doc.page_iter().nth(index).unwrap()
}

fn page_dict(doc: &Document, index: usize) -> Dictionary {
    doc.get_dictionary(page_id(doc, index)).unwrap().clone()
}

fn as_dict(doc: &Document, obj: &Object) -> Dictionary {
    let resolved = match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap(),
        Object::Null
        | Object::Boolean(_)
        | Object::Integer(_)
        | Object::Real(_)
        | Object::Name(_)
        | Object::String(_, _)
        | Object::Array(_)
        | Object::Dictionary(_)
        | Object::Stream(_) => obj,
    };
    resolved.as_dict().expect("dictionary").clone()
}

fn contents_array(doc: &Document, index: usize) -> Vec<Object> {
    page_dict(doc, index)
        .get(b"Contents")
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn stream_bytes(doc: &Document, obj: &Object) -> Vec<u8> {
    let id = obj.as_reference().unwrap();
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone())
}

fn font_bytes() -> Option<Vec<u8>> {
    let path = std::env::var_os("PDFCRAFT_FORMS_FONT")?;
    Some(fs::read(path).expect("read PDFCRAFT_FORMS_FONT"))
}

fn load_font(bytes: Vec<u8>) -> Font {
    if bytes.starts_with(b"ttcf") {
        Font::from_collection(bytes, 0).unwrap()
    } else {
        Font::new(bytes).unwrap()
    }
}

fn pdftotext_bbox(pdf: &[u8]) -> String {
    let mut child = Command::new("pdftotext")
        .args(["-bbox", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(pdf).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

struct WordBox {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
}

fn attr(tag: &str, key: &str) -> f32 {
    let needle = format!("{key}=\"");
    let from = tag.find(&needle).unwrap() + needle.len();
    let to = from + tag[from..].find('"').unwrap();
    tag[from..to].parse().unwrap()
}

fn parse_word_box(xml: &str, word: &str) -> WordBox {
    let marker = format!(">{word}</word>");
    let end = xml.find(&marker).expect(word);
    let start = xml[..end].rfind("<word ").expect("word tag");
    let tag = &xml[start..end];
    WordBox {
        x_min: attr(tag, "xMin"),
        y_min: attr(tag, "yMin"),
        x_max: attr(tag, "xMax"),
        y_max: attr(tag, "yMax"),
    }
}

fn dark_pixels(pdf: &[u8], page: u32, x: i32, y: i32, width: i32, height: i32) -> usize {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "pdfcraft-overlay-state-{}-{seq}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.pdf");
    fs::write(&input, pdf).unwrap();
    let status = Command::new("pdftoppm")
        .args([
            "-r",
            "72",
            "-gray",
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
            "-x",
            &x.to_string(),
            "-y",
            &y.to_string(),
            "-W",
            &width.to_string(),
            "-H",
            &height.to_string(),
            "-singlefile",
        ])
        .arg(&input)
        .arg(dir.join("out"))
        .status()
        .unwrap();
    assert!(status.success(), "pdftoppm failed");
    let pgm = fs::read(dir.join("out.pgm")).unwrap();
    fs::remove_dir_all(&dir).ok();
    pgm_body(&pgm).iter().filter(|byte| **byte < 128).count()
}

fn pgm_body(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    for _ in 0..3 {
        while rest.starts_with(b"#") {
            let nl = rest.iter().position(|byte| *byte == b'\n').unwrap();
            rest = &rest[nl + 1..];
        }
        let nl = rest.iter().position(|byte| *byte == b'\n').unwrap();
        rest = &rest[nl + 1..];
    }
    rest
}

fn is_one(obj: &Object) -> bool {
    match obj {
        Object::Integer(1) => true,
        Object::Real(value) => (*value - 1.0).abs() < f32::EPSILON,
        Object::Null
        | Object::Boolean(_)
        | Object::Integer(_)
        | Object::Name(_)
        | Object::String(_, _)
        | Object::Array(_)
        | Object::Dictionary(_)
        | Object::Stream(_)
        | Object::Reference(_) => false,
    }
}

fn is_name(obj: &Object, expected: &str) -> bool {
    obj.as_name().ok() == Some(expected.as_bytes())
}

#[test]
fn wraps_original_contents_for_every_contents_shape() {
    for shape in [
        ContentsShape::Stream,
        ContentsShape::DirectArray,
        ContentsShape::IndirectArray,
        ContentsShape::Missing,
    ] {
        let (input, original) = leaky_page_pdf(shape);
        let pdf = draw(&input, marks_request(), None).unwrap();
        let doc = load(&pdf);
        let contents = contents_array(&doc, 0);
        assert!(
            contents.len() == original.len() + 2,
            "shape contents len {}",
            contents.len()
        );
        let save = stream_bytes(&doc, &contents[0]);
        assert_eq!(
            std::str::from_utf8(&save)
                .unwrap()
                .split_whitespace()
                .collect::<String>(),
            "q"
        );
        let overlay = stream_bytes(&doc, contents.last().unwrap());
        let trimmed = overlay
            .iter()
            .copied()
            .skip_while(u8::is_ascii_whitespace)
            .collect::<Vec<_>>();
        assert!(
            trimmed.starts_with(b"Q"),
            "{}",
            String::from_utf8_lossy(&overlay)
        );
        assert_eq!(&contents[1..contents.len() - 1], original.as_slice());
        let before = load(&input);
        for item in &original {
            let Ok(id) = item.as_reference() else {
                continue;
            };
            let old = before
                .get_object(id)
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .clone();
            let new = doc
                .get_object(id)
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .clone();
            assert_eq!(old, new);
        }
    }
}

#[test]
fn rejects_contents_of_wrong_type() {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    set_page(
        &mut doc,
        page,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        Some(Object::Integer(5)),
        annots,
    );
    let input = finish(doc, page_tree, &[page]);
    let err = draw(&input, marks_request(), None).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::FlattenFailed(FlattenReason::AppendContent)
    );
}

#[test]
fn leaves_shared_contents_array_untouched() {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page1 = doc.new_object_id();
    let page2 = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    let a = stream_obj(&mut doc, LEAKY);
    let original = vec![Object::Reference(a)];
    let array_id = doc.add_object(Object::Array(original.clone()));
    let resources = resources_with_ext(extgstate_with(&[]));
    set_page(
        &mut doc,
        page1,
        page_tree,
        resources.clone(),
        Some(Object::Reference(array_id)),
        annots,
    );
    set_page(
        &mut doc,
        page2,
        page_tree,
        resources,
        Some(Object::Reference(array_id)),
        Vec::new(),
    );
    let input = finish(doc, page_tree, &[page1, page2]);
    let pdf = draw(&input, marks_request(), None).unwrap();
    let out = load(&pdf);
    let page2_id = page_id(&out, 1);
    let contents = out
        .get_dictionary(page2_id)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    assert_eq!(contents, Object::Reference(array_id));
    let array = out.get_object(array_id).unwrap().as_array().unwrap();
    assert_eq!(array, &original);
}

#[test]
fn untouched_pages_keep_contents_and_resources() {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page1 = doc.new_object_id();
    let page2 = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    let (contents1, _) = contents_object(&mut doc, ContentsShape::Stream);
    let idle = stream_obj(&mut doc, b"unmodified page two\n");
    let mut idle_res = Dictionary::new();
    idle_res.set("ProcSet", Object::Array(vec![name("PDF")]));
    set_page(
        &mut doc,
        page1,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        contents1,
        annots,
    );
    set_page(
        &mut doc,
        page2,
        page_tree,
        idle_res,
        Some(Object::Reference(idle)),
        Vec::new(),
    );
    let input = finish(doc, page_tree, &[page1, page2]);
    let before = load(&input);
    let before_page = page_dict(&before, 1);
    let pdf = draw(&input, marks_request(), None).unwrap();
    let after = load(&pdf);
    let after_page = page_dict(&after, 1);
    assert_eq!(
        before_page.get(b"Contents").ok(),
        after_page.get(b"Contents").ok()
    );
    assert_eq!(
        before_page.get(b"Resources").ok(),
        after_page.get(b"Resources").ok()
    );
}

#[test]
fn overlay_sets_opaque_before_any_paint() {
    let (input, _) = leaky_page_pdf(ContentsShape::Stream);
    let pdf = draw(&input, marks_request(), None).unwrap();
    let doc = load(&pdf);
    let contents = contents_array(&doc, 0);
    let overlay = stream_bytes(&doc, contents.last().unwrap());
    let decoded = lopdf::content::Content::decode(&overlay).unwrap();
    assert!(decoded.operations.len() >= 2, "{decoded:?}");
    assert_eq!(decoded.operations[0].operator, "Q");
    assert_eq!(decoded.operations[1].operator, "gs");
    let gs_name = decoded.operations[1].operands[0].as_name().unwrap();
    let resources = as_dict(&doc, page_dict(&doc, 0).get(b"Resources").unwrap());
    let ext = as_dict(&doc, resources.get(b"ExtGState").unwrap());
    assert!(
        ext.has(gs_name),
        "missing {}",
        String::from_utf8_lossy(gs_name)
    );
    let mut seen_gs = false;
    for op in &decoded.operations {
        if op.operator == "gs"
            && op
                .operands
                .first()
                .and_then(|operand| operand.as_name().ok())
                == Some(gs_name)
        {
            seen_gs = true;
        }
        if op.operator == "Tj" || op.operator == "S" {
            assert!(seen_gs, "paint before gs: {op:?}");
        }
    }
}

#[test]
fn opaque_extgstate_has_expected_entries() {
    let (input, _) = leaky_page_pdf(ContentsShape::Stream);
    let pdf = draw(&input, marks_request(), None).unwrap();
    let doc = load(&pdf);
    let resources = as_dict(&doc, page_dict(&doc, 0).get(b"Resources").unwrap());
    let ext = as_dict(&doc, resources.get(b"ExtGState").unwrap());
    assert!(ext.has(b"GSz"));
    let mut found = None;
    for (key, value) in &ext {
        if key == b"GSz" {
            continue;
        }
        found = Some(as_dict(&doc, value));
    }
    let opaque = found.expect("new ExtGState");
    let keys: std::collections::BTreeSet<&[u8]> =
        opaque.iter().map(|(key, _)| key.as_slice()).collect();
    let expected: std::collections::BTreeSet<&[u8]> = [
        b"Type".as_slice(),
        b"CA".as_slice(),
        b"ca".as_slice(),
        b"BM".as_slice(),
        b"SMask".as_slice(),
        b"AIS".as_slice(),
        b"OP".as_slice(),
        b"op".as_slice(),
    ]
    .into();
    assert_eq!(keys, expected);
    assert!(is_name(opaque.get(b"Type").unwrap(), "ExtGState"));
    assert!(is_one(opaque.get(b"CA").unwrap()));
    assert!(is_one(opaque.get(b"ca").unwrap()));
    assert!(is_name(opaque.get(b"BM").unwrap(), "Normal"));
    assert!(is_name(opaque.get(b"SMask").unwrap(), "None"));
    assert_eq!(opaque.get(b"AIS").unwrap(), &Object::Boolean(false));
    assert_eq!(opaque.get(b"OP").unwrap(), &Object::Boolean(false));
    assert_eq!(opaque.get(b"op").unwrap(), &Object::Boolean(false));
}

fn page_with_ext_names(names: &[&str], indirect: bool) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    let (contents, _) = contents_object(&mut doc, ContentsShape::Stream);
    let ext = extgstate_with(names);
    let resources = if indirect {
        let id = doc.add_object(Object::Dictionary(ext));
        let mut resources = Dictionary::new();
        resources.set("ExtGState", Object::Reference(id));
        resources
    } else {
        resources_with_ext(ext)
    };
    set_page(&mut doc, page, page_tree, resources, contents, annots);
    finish(doc, page_tree, &[page])
}

#[test]
fn opaque_name_avoids_existing_entries() {
    for indirect in [false, true] {
        let input = page_with_ext_names(&["PdfCraftOpaque"], indirect);
        let before = load(&input);
        let before_ext = {
            let resources = as_dict(&before, page_dict(&before, 0).get(b"Resources").unwrap());
            resources.get(b"ExtGState").unwrap().clone()
        };
        let before_indirect = match &before_ext {
            Object::Reference(id) => Some((*id, as_dict(&before, &before_ext))),
            Object::Null
            | Object::Boolean(_)
            | Object::Integer(_)
            | Object::Real(_)
            | Object::Name(_)
            | Object::String(_, _)
            | Object::Array(_)
            | Object::Dictionary(_)
            | Object::Stream(_) => None,
        };
        let pdf = draw(&input, marks_request(), None).unwrap();
        let doc = load(&pdf);
        let resources = as_dict(&doc, page_dict(&doc, 0).get(b"Resources").unwrap());
        let ext = as_dict(&doc, resources.get(b"ExtGState").unwrap());
        assert!(ext.has(b"PdfCraftOpaque"));
        assert!(ext.has(b"PdfCraftOpaque2"));
        let overlay = stream_bytes(&doc, contents_array(&doc, 0).last().unwrap());
        let text = String::from_utf8_lossy(&overlay);
        assert!(text.contains("/PdfCraftOpaque2 gs"), "{text}");
        let kept = as_dict(&doc, ext.get(b"PdfCraftOpaque").unwrap());
        assert_eq!(kept.get(b"ca").unwrap(), &Object::Integer(0));
        assert!(kept.get(b"Type").is_err());
        if let Some((id, old)) = before_indirect {
            match doc.get_object(id) {
                Ok(obj) => {
                    let still = obj.as_dict().unwrap();
                    assert_eq!(still, &old);
                    assert!(!still.has(b"PdfCraftOpaque2"));
                }
                Err(_) => {
                    assert!(matches!(
                        resources.get(b"ExtGState").unwrap(),
                        Object::Dictionary(_)
                    ));
                }
            }
        }
    }
}

#[test]
fn opaque_name_exhaustion_fails() {
    let mut names = vec!["PdfCraftOpaque".to_owned()];
    for n in 2..=64 {
        names.push(format!("PdfCraftOpaque{n}"));
    }
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let input = page_with_ext_names(&borrowed, false);
    let err = draw(&input, marks_request(), None).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::FlattenFailed(FlattenReason::GraphicsStateNameTaken)
    );
}

#[test]
fn marks_only_page_gets_opaque_resource() {
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let annots = vec![Object::Reference(widget(
        &mut doc, "agree", "Btn", CHECK_RECT,
    ))];
    let (contents, _) = contents_object(&mut doc, ContentsShape::Stream);
    set_page(
        &mut doc,
        page,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        contents,
        annots,
    );
    let input = finish(doc, page_tree, &[page]);
    let request = FillRequest::new(BTreeMap::from([(
        "agree".to_owned(),
        FieldFill::check(true),
    )]));
    let pdf = draw(&input, request, None).unwrap();
    let out = load(&pdf);
    let resources = as_dict(&out, page_dict(&out, 0).get(b"Resources").unwrap());
    let ext = as_dict(&out, resources.get(b"ExtGState").unwrap());
    assert!(ext.has(b"GSz"));
    assert!(ext.iter().any(|(key, _)| key != b"GSz"));
    assert!(resources.get(b"Font").is_err());
}

#[test]
fn leaky_page_marks_are_inked() {
    let (input, _) = leaky_page_pdf(ContentsShape::Stream);
    let pdf = draw(&input, marks_request(), None).unwrap();
    let check = dark_pixels(&pdf, 1, 21, 91, 28, 28);
    let rule = dark_pixels(&pdf, 1, 21, 53, 158, 3);
    let line = dark_pixels(&pdf, 1, 31, 159, 138, 3);
    assert!(check > 0, "checkbox dark pixels {check}");
    assert!(rule > 0, "rule dark pixels {rule}");
    assert!(line > 0, "line dark pixels {line}");
}

#[test]
fn leaky_page_text_is_inked_and_measured() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let (leaky, _) = leaky_page_pdf(ContentsShape::Stream);
    let filled = draw(&leaky, text_request(), Some(&font)).unwrap();
    let ink = dark_pixels(&filled, 1, 21, 21, 158, 28);
    assert!(ink > 0, "text dark pixels {ink}");
    let leaky_box = parse_word_box(&pdftotext_bbox(&filled), FILL_WORD);
    let mut clean_doc = Document::with_version("1.4");
    let page_tree = clean_doc.new_object_id();
    let page = clean_doc.new_object_id();
    let annots = text_and_check(&mut clean_doc);
    set_page(
        &mut clean_doc,
        page,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        None,
        annots,
    );
    let clean_input = finish(clean_doc, page_tree, &[page]);
    let clean = draw(&clean_input, text_request(), Some(&font)).unwrap();
    let clean_box = parse_word_box(&pdftotext_bbox(&clean), FILL_WORD);
    let leaky_w = leaky_box.x_max - leaky_box.x_min;
    let clean_w = clean_box.x_max - clean_box.x_min;
    assert!(
        (leaky_w - clean_w).abs() < 0.5,
        "leaky width {leaky_w} control {clean_w}"
    );
}

#[test]
fn cm_leak_does_not_move_text() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let mut doc = Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let annots = text_and_check(&mut doc);
    let contents = Object::Reference(stream_obj(&mut doc, CM_LEAK));
    set_page(
        &mut doc,
        page,
        page_tree,
        resources_with_ext(extgstate_with(&[])),
        Some(contents),
        annots,
    );
    let input = finish(doc, page_tree, &[page]);
    let pdf = draw(&input, text_request(), Some(&font)).unwrap();
    let word = parse_word_box(&pdftotext_bbox(&pdf), FILL_WORD);
    let left = word.x_min;
    let right = word.x_max;
    let bottom = f32::from(PAGE) - word.y_max;
    let top = f32::from(PAGE) - word.y_min;
    let slack = 4.0;
    assert!(
        left >= f32::from(TEXT_RECT[0]) - slack
            && right <= f32::from(TEXT_RECT[2]) + slack
            && bottom >= f32::from(TEXT_RECT[1]) - slack
            && top <= f32::from(TEXT_RECT[3]) + slack
            && left < f32::from(TEXT_RECT[0]) * 2.0,
        "bbox pdf=({left},{bottom})-({right},{top}) raw=({} {} {} {})",
        word.x_min,
        word.y_min,
        word.x_max,
        word.y_max
    );
}

#[test]
fn leaky_fill_is_deterministic() {
    let (input, _) = leaky_page_pdf(ContentsShape::Stream);
    let form = Form::open(&input).unwrap();
    let fill = Fill::new(&form, marks_request(), None).unwrap();
    let first = fill.to_pdf().unwrap();
    let second = fill.to_pdf().unwrap();
    assert_eq!(first, second);
}
