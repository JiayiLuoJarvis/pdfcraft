//! Integration tests for [`pdfcraft_post::first_page_jpeg`].
//!
//! First-page-only rendering, longest-side capping, JPEG validity,
//! empty/encrypted/non-PDF inputs, and quality/size sanity.

#![cfg(feature = "thumbnail")]
#![allow(
    clippy::similar_names,
    reason = "fixture builder uses page/pages ids like tests/common"
)]

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use std::fs;
use std::path::PathBuf;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdfcraft_post::{first_page_jpeg, Encrypt, EncryptionPreset, ErrorKind, ThumbnailOptions};

/// One fixture page: text `mark`, a solid fill `color`, and a filled rect.
struct FixturePage<'a> {
    mark: &'a str,
    mediabox: [i64; 4],
    color: [i64; 3],
    rect: [i64; 4],
}

/// Build a PDF whose pages render visibly different content.
///
/// Each page sets Helvetica text plus a solid-color filled rectangle, so two
/// pages with different marks also differ in rendered pixels. `mediabox` is
/// `[x0, y0, x1, y1]` in points.
fn pdf_with_fixture_pages(pages: &[FixturePage<'_>]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    let mut kids = Vec::new();
    for page in pages {
        let text_y = page.mediabox[3] - page.mediabox[1] - 60;
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 48.into()]),
                Operation::new("Td", vec![20.into(), text_y.into()]),
                Operation::new("Tj", vec![Object::string_literal(page.mark)]),
                Operation::new("ET", vec![]),
                Operation::new(
                    "rg",
                    vec![
                        page.color[0].into(),
                        page.color[1].into(),
                        page.color[2].into(),
                    ],
                ),
                Operation::new(
                    "re",
                    vec![
                        page.rect[0].into(),
                        page.rect[1].into(),
                        page.rect[2].into(),
                        page.rect[3].into(),
                    ],
                ),
                Operation::new("f", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode"),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![
                page.mediabox[0].into(),
                page.mediabox[1].into(),
                page.mediabox[2].into(),
                page.mediabox[3].into(),
            ],
        });
        kids.push(page_id.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::try_from(pages.len()).expect("page count"),
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

fn square(mark: &str, color: [i64; 3], rect: [i64; 4]) -> FixturePage<'_> {
    FixturePage {
        mark,
        mediabox: [0, 0, 200, 200],
        color,
        rect,
    }
}

/// Decode `jpeg` and return its `(width, height)` in pixels.
fn jpeg_dimensions(jpeg: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory(jpeg).expect("thumbnail must decode");
    (image.width(), image.height())
}

fn assert_is_jpeg(jpeg: &[u8]) {
    assert!(
        jpeg.len() >= 2 && jpeg[0] == 0xFF && jpeg[1] == 0xD8,
        "thumbnail must start with JPEG magic FF D8"
    );
    let (width, height) = jpeg_dimensions(jpeg);
    assert!(
        width > 0 && height > 0,
        "decoded thumbnail must have nonzero dimensions"
    );
}

#[test]
fn first_page_only_thumb_ignores_later_pages() {
    let red = [1, 0, 0];
    let blue = [0, 0, 1];
    let single = pdf_with_fixture_pages(&[square("alpha", red, [20, 20, 100, 100])]);
    let multi = pdf_with_fixture_pages(&[
        square("alpha", red, [20, 20, 100, 100]),
        square("beta", blue, [60, 60, 100, 100]),
        square("gamma", blue, [20, 60, 100, 100]),
    ]);
    let opts = ThumbnailOptions::default();
    let from_single = first_page_jpeg(&single, &opts)
        .expect("single-page render")
        .expect("single-page thumb");
    let from_multi = first_page_jpeg(&multi, &opts)
        .expect("multi-page render")
        .expect("multi-page thumb");
    assert_is_jpeg(&from_single);
    assert_is_jpeg(&from_multi);
    assert_eq!(
        from_single, from_multi,
        "thumbnail must be stable regardless of page count"
    );
}

#[test]
fn differing_second_page_never_leaks_into_thumb() {
    // QA probe: a document holding only the second page's mark renders
    // differently, yet the multi-page thumbnail still equals the page-1
    // render, proving only the first page is read.
    let red = [1, 0, 0];
    let blue = [0, 0, 1];
    let first_only = pdf_with_fixture_pages(&[square("alpha", red, [20, 20, 100, 100])]);
    let second_only = pdf_with_fixture_pages(&[square("beta", blue, [60, 60, 100, 100])]);
    let multi = pdf_with_fixture_pages(&[
        square("alpha", red, [20, 20, 100, 100]),
        square("beta", blue, [60, 60, 100, 100]),
    ]);
    let opts = ThumbnailOptions::default();
    let thumb = |pdf: &[u8]| first_page_jpeg(pdf, &opts).expect("render").expect("thumb");
    let page1 = thumb(&first_only);
    let page2 = thumb(&second_only);
    assert_ne!(
        page1, page2,
        "fixture marks must render visibly different pages"
    );
    assert_eq!(
        thumb(&multi),
        page1,
        "multi-page thumbnail must equal the page-1 render"
    );
}

#[test]
fn longest_side_capped_on_tall_and_wide_pages() {
    let red = [1, 0, 0];
    let opts = ThumbnailOptions::default();
    assert_eq!(opts.max_longest_side, 320, "default longest side");
    let tall = pdf_with_fixture_pages(&[FixturePage {
        mark: "tall",
        mediabox: [0, 0, 200, 800],
        color: red,
        rect: [20, 20, 100, 100],
    }]);
    let wide = pdf_with_fixture_pages(&[FixturePage {
        mark: "wide",
        mediabox: [0, 0, 800, 200],
        color: red,
        rect: [20, 20, 100, 100],
    }]);
    let tall_jpeg = first_page_jpeg(&tall, &opts)
        .expect("tall render")
        .expect("tall thumb");
    let wide_jpeg = first_page_jpeg(&wide, &opts)
        .expect("wide render")
        .expect("wide thumb");
    assert_is_jpeg(&tall_jpeg);
    assert_is_jpeg(&wide_jpeg);
    let (tall_w, tall_h) = jpeg_dimensions(&tall_jpeg);
    let (wide_w, wide_h) = jpeg_dimensions(&wide_jpeg);
    assert!(
        tall_h > tall_w,
        "tall page must stay portrait: {tall_w}x{tall_h}"
    );
    assert!(
        wide_w > wide_h,
        "wide page must stay landscape: {wide_w}x{wide_h}"
    );
    assert!(
        tall_w.max(tall_h) <= 320,
        "tall longest side capped: {tall_w}x{tall_h}"
    );
    assert!(
        wide_w.max(wide_h) <= 320,
        "wide longest side capped: {wide_w}x{wide_h}"
    );
}

#[test]
fn empty_pdf_yields_none() {
    let empty = common::pages_with_marks(&[]);
    let doc = Document::load_mem(&empty).expect("empty fixture must load");
    assert_eq!(doc.get_pages().len(), 0, "fixture holds no pages");
    let out = first_page_jpeg(&empty, &ThumbnailOptions::default()).expect("empty input");
    assert!(out.is_none(), "a PDF with no pages yields None");
}

#[test]
fn encrypted_with_empty_user_is_rejected() {
    let plain = pdf_with_fixture_pages(&[square("secret", [1, 0, 0], [20, 20, 100, 100])]);
    let enc = common::encrypted_with_empty_user(&plain);
    let direct = Document::load_mem(&enc).expect("fixture auto-decrypts");
    assert!(
        direct.was_encrypted(),
        "fixture must exercise the was_encrypted branch"
    );
    let err = first_page_jpeg(&enc, &ThumbnailOptions::default()).expect_err("encrypted input");
    assert_eq!(err.kind, ErrorKind::Encrypted);
}

#[test]
fn encrypted_with_user_password_is_rejected() {
    let plain = pdf_with_fixture_pages(&[square("locked", [1, 0, 0], [20, 20, 100, 100])]);
    let enc = Encrypt {
        config: Some(EncryptionPreset::READ_ONLY),
        owner_password: Some("owner-secret".into()),
        user_password: Some("user-secret".into()),
    }
    .apply(&plain)
    .expect("encrypt fixture");
    let err =
        first_page_jpeg(&enc, &ThumbnailOptions::default()).expect_err("password-locked input");
    assert_eq!(err.kind, ErrorKind::Encrypted);
}

#[test]
fn non_pdf_is_rejected() {
    let seed =
        fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/seeds/p03-not-a-pdf.bin"))
            .expect("read p03 seed");
    assert!(
        !seed.starts_with(b"%PDF"),
        "p03 seed must not look like a PDF"
    );
    let err = first_page_jpeg(&seed, &ThumbnailOptions::default()).expect_err("p03-not-a-pdf.bin");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}

#[test]
fn higher_quality_is_not_smaller_and_stays_bounded() {
    let pdf = pdf_with_fixture_pages(&[square("quality", [1, 0, 0], [20, 20, 100, 100])]);
    let mut low = ThumbnailOptions::default();
    low.quality = 10;
    let mut high = ThumbnailOptions::default();
    high.quality = 90;
    let low_jpeg = first_page_jpeg(&pdf, &low)
        .expect("q10 render")
        .expect("q10 thumb");
    let high_jpeg = first_page_jpeg(&pdf, &high)
        .expect("q90 render")
        .expect("q90 thumb");
    assert_is_jpeg(&low_jpeg);
    assert_is_jpeg(&high_jpeg);
    assert!(
        high_jpeg.len() >= low_jpeg.len(),
        "q90 ({} bytes) must not be smaller than q10 ({} bytes)",
        high_jpeg.len(),
        low_jpeg.len()
    );
    assert!(
        high_jpeg.len() < 10 * low_jpeg.len(),
        "q90 ({} bytes) must stay bounded vs q10 ({} bytes)",
        high_jpeg.len(),
        low_jpeg.len()
    );
}

#[test]
fn zero_max_side_fails() {
    let pdf = pdf_with_fixture_pages(&[square("zero", [1, 0, 0], [20, 20, 100, 100])]);
    let mut opts = ThumbnailOptions::default();
    opts.max_longest_side = 0;
    let err = first_page_jpeg(&pdf, &opts).expect_err("zero longest side");
    assert_eq!(err.kind, ErrorKind::ThumbnailFailed);
}
