use super::*;

fn limits() -> OwnerRootBindingLimits {
    OwnerRootBindingLimits::new(64, 1024).unwrap()
}
fn id() -> ProjectId {
    "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap()
}
#[test]
fn both_os_data_goldens_keep_every_native_identity_field() {
    // These complete literals are independent of the encoder and decoder.
    // Pure Windows DATA on Unix is never native Windows qualification.
    const UNIX: [u8; 71] = [
        0x47, 0x4f, 0x52, 0x4f, 0x4f, 0x54, 0x30, 0x31, 0x01, 0x00, 0x01, 0x30, 0x31, 0x41, 0x52,
        0x5a, 0x33, 0x4e, 0x44, 0x45, 0x4b, 0x54, 0x53, 0x56, 0x34, 0x52, 0x52, 0x46, 0x46, 0x51,
        0x36, 0x39, 0x47, 0x35, 0x46, 0x41, 0x56, 0x07, 0x00, 0x00, 0x00, 0x67, 0x72, 0x61, 0x70,
        0x68, 0x69, 0x74, 0x01, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x18, 0x17, 0x16,
        0x15, 0x14, 0x13, 0x12, 0x11, 0x01, 0x01, 0xe8, 0x01, 0x00, 0x00,
    ];
    const WINDOWS: [u8; 84] = [
        0x47, 0x4f, 0x52, 0x4f, 0x4f, 0x54, 0x30, 0x31, 0x01, 0x00, 0x01, 0x30, 0x31, 0x41, 0x52,
        0x5a, 0x33, 0x4e, 0x44, 0x45, 0x4b, 0x54, 0x53, 0x56, 0x34, 0x52, 0x52, 0x46, 0x46, 0x51,
        0x36, 0x39, 0x47, 0x35, 0x46, 0x41, 0x56, 0x07, 0x00, 0x00, 0x00, 0x67, 0x72, 0x61, 0x70,
        0x68, 0x69, 0x74, 0x02, 0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21, 0x40, 0x41, 0x42,
        0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x38, 0x37,
        0x36, 0x35, 0x34, 0x33, 0x32, 0x31, 0x01, 0x02, 0x01,
    ];
    let cases = [
        (
            PhysicalId::Unix {
                device: 0x0102030405060708,
                inode: 0x1112131415161718,
            },
            OwnerRootMode::Unix(0o750),
            UNIX.as_slice(),
        ),
        (
            PhysicalId::Windows {
                volume: 0x2122232425262728,
                index: [
                    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c,
                    0x4d, 0x4e, 0x4f,
                ],
                created: 0x3132333435363738,
            },
            OwnerRootMode::Windows { readonly: true },
            WINDOWS.as_slice(),
        ),
    ];
    for (identity, mode, golden) in cases {
        let decoded = OwnerRootBindingRecord::decode(golden, limits()).unwrap();
        assert_eq!(decoded.project_id(), id());
        assert_eq!(decoded.brand(), "graphit");
        assert!(decoded.identity == identity);
        assert!(decoded.root_mode() == mode);
        assert_eq!(decoded.wire(), golden);
        let encoded =
            OwnerRootBindingRecord::materialize(id(), "graphit", identity, mode, limits()).unwrap();
        assert_eq!(encoded.wire(), golden);
    }
}
#[test]
fn root_data_boundaries_are_exact_and_invalid_wire_is_not_a_capability() {
    let value = OwnerRootBindingRecord::materialize(
        id(),
        "graphit",
        PhysicalId::Unix {
            device: 0,
            inode: 1,
        },
        OwnerRootMode::Unix(0o700),
        limits(),
    )
    .unwrap();
    let exact = OwnerRootBindingLimits::new(7, value.wire().len()).unwrap();
    assert!(OwnerRootBindingRecord::decode(value.wire(), exact).is_ok());
    assert!(
        OwnerRootBindingRecord::decode(value.wire(), OwnerRootBindingLimits::new(6, 1024).unwrap())
            .is_err()
    );
    assert!(
        OwnerRootBindingRecord::decode(
            value.wire(),
            OwnerRootBindingLimits::new(7, value.wire().len() - 1).unwrap()
        )
        .is_err()
    );
    for length in 0..value.wire().len() {
        assert!(OwnerRootBindingRecord::decode(&value.wire()[..length], limits()).is_err());
    }
    for offset in [0, 8, 10, 37, 48, 65, 66] {
        let mut invalid = value.wire().to_vec();
        invalid[offset] = 0xff;
        assert!(OwnerRootBindingRecord::decode(&invalid, limits()).is_err());
    }
    let mut trailing = value.wire().to_vec();
    trailing.push(0);
    assert!(OwnerRootBindingRecord::decode(&trailing, limits()).is_err());
    assert!(OwnerRootBindingLimits::new(0, 1024).is_err());
    assert!(OwnerRootBindingLimits::new(64, 0).is_err());
}
