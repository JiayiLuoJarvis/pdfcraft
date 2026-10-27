//! Integration tests for pdfcraft-post.

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use lopdf::Object;
use pdfcraft_post::{
    Encrypt, EncryptionPreset, ErrorKind, Pipeline, SetMetadata, StampAndEncrypt, Step,
};

#[test]
fn stamp_and_encrypt_sets_title_and_encrypts() {
    let pdf = common::sample_pdf(None, None);
    let out = StampAndEncrypt {
        metadata: SetMetadata {
            title: Some("Stamped".into()),
            ..SetMetadata::default()
        },
        encrypt: Encrypt {
            config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
            owner_password: Some("owner-secret".into()),
            user_password: None,
        },
    }
    .apply(&pdf)
    .expect("stamp");

    let doc = common::open_with_password(&out, "").expect("open");
    assert!(common::is_encrypted_pdf(&out));
    let info_ref = doc.trailer.get(b"Info").expect("info");
    let id = info_ref.as_reference().expect("ref");
    let info = doc.get_object(id).expect("obj").as_dict().expect("dict");
    let Object::String(bytes, _) = info.get(b"Title").expect("title") else {
        panic!("title not string");
    };
    assert_eq!(String::from_utf8_lossy(bytes), "Stamped");
}

#[test]
fn missing_encrypt_config_on_combo() {
    let pdf = common::sample_pdf(None, None);
    let err = StampAndEncrypt {
        metadata: SetMetadata::default(),
        encrypt: Encrypt::default(),
    }
    .apply(&pdf)
    .expect_err("missing");
    assert_eq!(err.kind, ErrorKind::ConfigMissing);
}

#[test]
fn pipeline_metadata_then_encrypt_also_works() {
    let pdf = common::sample_pdf(None, None);
    let out = Pipeline::new([
        Step::Metadata(SetMetadata {
            title: Some("ViaPipe".into()),
            ..SetMetadata::default()
        }),
        Step::Encrypt(Encrypt {
            config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
            owner_password: Some("owner-secret".into()),
            user_password: None,
        }),
    ])
    .run(&pdf)
    .expect("pipe");
    let doc = common::open_with_password(&out, "").expect("open");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(common::is_encrypted_pdf(&out));
    let info_id = doc
        .trailer
        .get(b"Info")
        .expect("info")
        .as_reference()
        .expect("ref");
    let info = doc
        .get_object(info_id)
        .expect("obj")
        .as_dict()
        .expect("dict");
    let Object::String(bytes, _) = info.get(b"Title").expect("title") else {
        panic!("title not string");
    };
    assert_eq!(String::from_utf8_lossy(bytes), "ViaPipe");
}
