use graphit_ast::{BytePoint, RangeError, SourceText};

fn point(row: usize, column: usize) -> BytePoint {
    BytePoint { row, column }
}

fn source(text: &str) -> SourceText<'_> {
    SourceText::new(text.as_bytes(), text.len().max(1)).unwrap()
}

#[test]
fn f01_empty() {
    let text = SourceText::new(&[], 1).unwrap();
    assert_eq!(text.point_at(0), Ok(point(0, 0)));
    assert_eq!(text.byte_at(point(0, 0)), Ok(0));
    assert_eq!(text.scalar_byte(0), Ok(0));
    assert_eq!(text.range(0, 0).unwrap().text(), "");
}

#[test]
fn f02_lf() {
    let text = source("a\nb");
    for (byte, expected) in [(1, point(0, 1)), (2, point(1, 0)), (3, point(1, 1))] {
        assert_eq!(text.point_at(byte), Ok(expected));
        assert_eq!(text.byte_at(expected), Ok(byte));
    }
    assert_eq!(text.range(0, 2).unwrap().text(), "a\n");
    assert_eq!(text.byte_at(point(0, 2)), Err(RangeError::InvalidPoint));
}

#[test]
fn f03_crlf_and_lone_cr() {
    let original = "a\r\nb\r";
    let text = source(original);
    for (byte, expected) in [(2, point(0, 2)), (3, point(1, 0)), (5, point(1, 2))] {
        assert_eq!(text.point_at(byte), Ok(expected));
        assert_eq!(text.byte_at(expected), Ok(byte));
    }
    assert_eq!(
        text.range(0, 5).unwrap().text().as_bytes(),
        original.as_bytes()
    );
}

#[test]
fn f04_unicode_and_bom() {
    let original = "é中🦀e\u{0301}";
    assert_eq!(original.len(), 12);
    let text = source(original);
    for byte in [0, 2, 5, 9, 10, 12] {
        assert_eq!(text.point_at(byte), Ok(point(0, byte)));
    }
    let bom = "\u{feff}a";
    let text = source(bom);
    assert_eq!(
        text.range(0, 4).unwrap().text().as_bytes(),
        &[0xef, 0xbb, 0xbf, 0x61]
    );
    for byte in [0, 3, 4] {
        assert_eq!(text.point_at(byte), Ok(point(0, byte)));
    }
}

#[test]
fn f05_invalid_utf8() {
    for (bytes, valid_up_to, error_len) in [
        (&[0xc3][..], 0, None),
        (&[0x80][..], 0, Some(1)),
        (&[0x61, 0xff][..], 1, Some(1)),
        (&[0xed, 0xa0, 0x80][..], 0, Some(1)),
    ] {
        assert_eq!(
            SourceText::new(bytes, bytes.len()).err(),
            Some(RangeError::InvalidUtf8 {
                valid_up_to,
                error_len
            })
        );
    }
}

#[test]
fn f06_invalid_endpoints_and_precedence() {
    let text = source("éx");
    assert_eq!(text.point_at(1), Err(RangeError::NotCharBoundary));
    assert_eq!(text.point_at(4), Err(RangeError::OutOfBounds));
    assert_eq!(text.byte_at(point(0, 1)), Err(RangeError::NotCharBoundary));
    for p in [point(0, 4), point(1, 0)] {
        assert_eq!(text.byte_at(p), Err(RangeError::InvalidPoint));
    }
    for (start, end, error) in [
        (2, 1, RangeError::ReversedRange),
        (0, 4, RangeError::OutOfBounds),
        (1, 2, RangeError::NotCharBoundary),
        (1, 4, RangeError::OutOfBounds),
        (usize::MAX, 0, RangeError::ReversedRange),
    ] {
        assert_eq!(text.range(start, end).err(), Some(error));
    }
    assert_eq!(text.range(3, 3).unwrap().text(), "");
}

#[test]
fn f07_original_scalar_ordinals() {
    let text = source("é中🦀e\u{0301}");
    for (index, byte) in [0, 2, 5, 9, 10, 12].into_iter().enumerate() {
        assert_eq!(text.scalar_byte(index), Ok(byte));
    }
    assert_eq!(text.scalar_byte(6), Err(RangeError::OutOfBounds));
    let text = source("\u{feff}a");
    for (index, byte) in [0, 3, 4].into_iter().enumerate() {
        assert_eq!(text.scalar_byte(index), Ok(byte));
    }
    assert_eq!(source("🦀").scalar_byte(1), Ok(4));
}

#[test]
fn f08_boundary_roundtrips_and_original_slices() {
    for original in [
        "",
        "ascii",
        "é中🦀e\u{0301}",
        "\u{feff}a",
        "é\r\n🦀\n",
        "\n\n",
    ] {
        let text = source(original);
        for byte in 0..=original.len() {
            if original.is_char_boundary(byte) {
                let p = text.point_at(byte).unwrap();
                assert_eq!(text.byte_at(p), Ok(byte));
            } else {
                assert_eq!(text.point_at(byte), Err(RangeError::NotCharBoundary));
            }
        }
        for (byte, ch) in original.char_indices() {
            let end = byte + ch.len_utf8();
            assert_eq!(text.range(byte, end).unwrap().text(), &original[byte..end]);
        }
    }
}

#[test]
fn f09_extreme_integers() {
    for original in ["", "a\n", "éx"] {
        let text = source(original);
        assert_eq!(text.point_at(usize::MAX), Err(RangeError::OutOfBounds));
        assert_eq!(text.scalar_byte(usize::MAX), Err(RangeError::OutOfBounds));
        assert_eq!(
            text.range(0, usize::MAX).err(),
            Some(RangeError::OutOfBounds)
        );
        for p in [point(usize::MAX, 0), point(0, usize::MAX)] {
            assert_eq!(text.byte_at(p), Err(RangeError::InvalidPoint));
        }
    }
}

#[test]
fn f10_input_limits_and_precedence() {
    assert!(SourceText::new("é".as_bytes(), 2).is_ok());
    assert_eq!(
        SourceText::new("é".as_bytes(), 1).err(),
        Some(RangeError::InputTooLarge)
    );
    assert_eq!(
        SourceText::new("é".as_bytes(), 0).err(),
        Some(RangeError::InvalidLimit)
    );
    assert_eq!(
        SourceText::new(&[0xff], 0).err(),
        Some(RangeError::InvalidLimit)
    );
    assert_eq!(
        SourceText::new(&[0xff, 0xff], 1).err(),
        Some(RangeError::InputTooLarge)
    );
}

#[test]
fn f11_borrowed_identity_and_wrapper_lifetime() {
    let original = String::from("aé\r\n🦀x\n");
    let range = {
        let text = source(&original);
        text.range(5, 9).unwrap()
    };
    assert_eq!(range.text(), "🦀");
    assert_eq!(range.text().as_ptr(), original[5..9].as_ptr());
    assert_eq!(range.start(), 5);
    assert_eq!(range.end(), 9);
    assert_eq!(range.start_point(), point(1, 0));
    assert_eq!(range.end_point(), point(1, 4));
}

#[test]
fn f12_final_lf_and_eof_range() {
    let text = source("x\n");
    let range = text.range(0, 2).unwrap();
    assert_eq!(range.text(), "x\n");
    assert_eq!(range.start_point(), point(0, 0));
    assert_eq!(range.end_point(), point(1, 0));
    let eof = text.range(2, 2).unwrap();
    assert_eq!(eof.text(), "");
    assert_eq!(eof.start_point(), point(1, 0));
    assert_eq!(eof.end_point(), point(1, 0));
}
