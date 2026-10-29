//! Shared fixture helpers for integration tests.

#![allow(
    clippy::similar_names,
    reason = "fixture helpers use short similar names"
)]

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, LoadOptions, Object, Stream};

/// Build a one-page PDF with optional `/Info` Title and Creator.
pub fn sample_pdf(title: Option<&str>, creator: Option<&str>) -> Vec<u8> {
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
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("Tj", vec![Object::string_literal("pdfcraft-post")]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut info = lopdf::Dictionary::new();
    if let Some(title) = title {
        info.set("Title", Object::string_literal(title));
    }
    if let Some(creator) = creator {
        info.set("Creator", Object::string_literal(creator));
    }
    if !info.is_empty() {
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", info_id);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

/// Read a literal string `/Info` field, if present.
pub fn info_string(pdf: &[u8], key: &[u8]) -> Option<String> {
    let doc = Document::load_mem(pdf).ok()?;
    let info_ref = doc.trailer.get(b"Info").ok()?;
    let Object::Reference(id) = info_ref else {
        return None;
    };
    let info = doc.get_object(*id).ok()?.as_dict().ok()?;
    let Object::String(bytes, _) = info.get(key).ok()? else {
        return None;
    };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Open encrypted PDF bytes with a user password.
pub fn open_with_password(pdf: &[u8], password: &str) -> Result<Document, lopdf::Error> {
    Document::load_mem_with_options(pdf, LoadOptions::with_password(password))
}

/// Encrypt `pdf` with an owner password and an empty user password.
///
/// Anyone can open the output, so `Document::load_mem` auto-decrypts it:
/// `is_encrypted()` is false while `was_encrypted()` is true. Fixtures from
/// here anchor the `was_encrypted` gate branch instead of the `Decryption`
/// error path.
pub fn encrypted_with_empty_user(pdf: &[u8]) -> Vec<u8> {
    pdfcraft_post::Encrypt {
        config: Some(pdfcraft_post::EncryptionPreset::READ_ONLY),
        owner_password: Some("owner-secret".into()),
        user_password: None,
    }
    .apply(pdf)
    .expect("encrypt fixture")
}

/// True when the trailer declares `/Encrypt`.
pub fn is_encrypted_pdf(pdf: &[u8]) -> bool {
    Document::load_metadata_mem(pdf).is_ok_and(|meta| meta.encrypted)
}

/// `/P` from the cleartext encrypt dictionary.
///
/// `load_mem` drops `/Encrypt` when the empty user password opens the file,
/// so the bit test reads the saved bytes.
pub fn permission_integer(pdf: &[u8]) -> i64 {
    let marker = b"/P ";
    let idx = pdf
        .windows(marker.len())
        .rposition(|window| window == marker)
        .expect("/P");
    let rest = &pdf[idx + marker.len()..];
    let end = rest
        .iter()
        .position(|byte| !byte.is_ascii_digit() && *byte != b'-')
        .expect("end of /P");
    std::str::from_utf8(&rest[..end])
        .expect("utf8")
        .parse()
        .expect("integer")
}

/// PDF permission bit in `/P` (bit 0 is the least significant bit).
pub fn perm_bit(pdf: &[u8], bit: u32) -> bool {
    permission_integer(pdf) & (1_i64 << bit) != 0
}

/// One page per mark. The mark is the page's `Tj` operand.
pub fn pages_with_marks(marks: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for mark in marks {
        let content = Content {
            operations: vec![Operation::new(
                "Tj",
                vec![Object::string_literal((*mark).to_string())],
            )],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        });
        kids.push(page_id.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::try_from(marks.len()).unwrap(),
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// One-page PDF whose outline root item is `title`.
pub fn pdf_with_outline(title: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.add_bookmark(
        lopdf::Bookmark::new(title.to_string(), [0.0, 0.0, 0.0], 0, page_id),
        None,
    );
    let outline_id = doc.build_outline().expect("outline");
    doc.catalog_mut()
        .expect("catalog")
        .set("Outlines", Object::Reference(outline_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// Titles of the top-level outline items, in order.
pub fn outline_titles(pdf: &[u8]) -> Vec<String> {
    let doc = Document::load_mem(pdf).expect("load");
    let Ok(outlines) = doc.catalog().expect("catalog").get(b"Outlines") else {
        return Vec::new();
    };
    let Ok(mut id) = outlines.as_reference() else {
        return Vec::new();
    };
    let Ok(root) = doc.get_dictionary(id) else {
        return Vec::new();
    };
    let Ok(first) = root.get(b"First").and_then(Object::as_reference) else {
        return Vec::new();
    };
    id = first;
    let mut titles = Vec::new();
    loop {
        let dict = doc.get_dictionary(id).expect("outline item");
        titles.push(lopdf::decode_text_string(dict.get(b"Title").expect("title")).expect("decode"));
        let Ok(next) = dict.get(b"Next").and_then(Object::as_reference) else {
            break;
        };
        id = next;
    }
    titles
}

/// Page content streams in reading order.
pub fn page_texts(pdf: &[u8]) -> Vec<String> {
    let doc = Document::load_mem(pdf).expect("load");
    doc.page_iter()
        .map(|id| String::from_utf8_lossy(&doc.get_page_content(id)).into_owned())
        .collect()
}
