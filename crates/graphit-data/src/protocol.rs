//! Delegates wire parsing to the published generated-format protocol.
use crate::{
    error::{DataReadError, Result},
    policy::LocalGraphLimits,
};
use polars_parquet_format::thrift::{
    self, ProtocolError, ProtocolErrorKind,
    protocol::{
        ReadThrift, TCompactInputProtocol, TFieldIdentifier, TInputProtocol, TListIdentifier,
        TMapIdentifier, TMessageIdentifier, TSetIdentifier, TStructIdentifier, TType,
    },
};
use std::io::{Cursor, Read};

pub(crate) struct Guard<R: Read> {
    inner: TCompactInputProtocol<R>,
    maximum: usize,
    aggregate: usize,
    used: usize,
    fields: Vec<Vec<i16>>,
    wire_type: Option<TType>,
}
fn failure() -> thrift::Error {
    thrift::Error::Protocol(ProtocolError {
        kind: ProtocolErrorKind::SizeLimit,
        message: "bounded metadata protocol".into(),
    })
}
impl<R: Read> Guard<R> {
    pub(crate) fn new(reader: R, limits: LocalGraphLimits) -> Self {
        Self {
            inner: TCompactInputProtocol::new(reader, limits.max_protocol_bytes),
            maximum: limits.max_list_entries,
            aggregate: limits.max_protocol_entries,
            used: 0,
            fields: Vec::new(),
            wire_type: None,
        }
    }
    fn typed(&self, expected: TType) -> thrift::Result<()> {
        if self.wire_type.is_some_and(|actual| actual != expected) {
            return Err(failure());
        }
        Ok(())
    }
    fn container(&mut self, count: u32, factor: usize) -> thrift::Result<()> {
        if count > i32::MAX as u32 {
            return Err(failure());
        }
        let count = usize::try_from(count).map_err(|_| failure())?;
        if count > self.maximum {
            return Err(failure());
        }
        self.used = count
            .checked_mul(factor)
            .and_then(|n| n.checked_add(self.used))
            .filter(|n| *n <= self.aggregate)
            .ok_or_else(failure)?;
        Ok(())
    }
}
macro_rules! scalar {
    ($($name:ident -> $return:ty, $wire:ident;)*) => {$ (
        fn $name(&mut self) -> thrift::Result<$return> {
            self.typed(TType::$wire)?; self.inner.$name()
        }
    )*};
}
impl<R: Read> TInputProtocol for Guard<R> {
    scalar! {
        read_bool -> bool, Bool; read_i8 -> i8, I08; read_i16 -> i16, I16;
        read_i32 -> i32, I32; read_i64 -> i64, I64; read_double -> f64, Double;
        read_byte -> u8, I08;
    }
    fn read_message_begin(&mut self) -> thrift::Result<TMessageIdentifier> {
        self.inner.read_message_begin()
    }
    fn read_message_end(&mut self) -> thrift::Result<()> {
        self.inner.read_message_end()
    }
    fn read_field_end(&mut self) -> thrift::Result<()> {
        self.wire_type = None;
        self.inner.read_field_end()
    }
    fn read_list_end(&mut self) -> thrift::Result<()> {
        self.wire_type = None;
        self.inner.read_list_end()
    }
    fn read_set_end(&mut self) -> thrift::Result<()> {
        self.wire_type = None;
        self.inner.read_set_end()
    }
    fn read_map_end(&mut self) -> thrift::Result<()> {
        self.wire_type = None;
        self.inner.read_map_end()
    }
    fn read_bytes(&mut self) -> thrift::Result<Vec<u8>> {
        self.typed(TType::String)?;
        let bytes = self.inner.read_bytes()?;
        Ok(bytes)
    }
    fn read_string(&mut self) -> thrift::Result<String> {
        self.typed(TType::String)?;
        let string = self.inner.read_string()?;
        Ok(string)
    }
    fn read_struct_begin(&mut self) -> thrift::Result<Option<TStructIdentifier>> {
        self.typed(TType::Struct)?;
        self.wire_type = None;
        if self.fields.len() >= 64 {
            return Err(failure());
        }
        self.fields.try_reserve(1).map_err(|_| failure())?;
        self.fields.push(Vec::new());
        self.inner.read_struct_begin()
    }
    fn read_struct_end(&mut self) -> thrift::Result<()> {
        if self.fields.pop().is_none() {
            return Err(failure());
        }
        self.wire_type = None;
        self.inner.read_struct_end()
    }
    fn read_field_begin(&mut self) -> thrift::Result<TFieldIdentifier> {
        let field = self.inner.read_field_begin()?;
        self.wire_type = Some(field.field_type);
        if field.field_type != TType::Stop {
            let ids = self.fields.last_mut().ok_or_else(failure)?;
            let id = field.id.ok_or_else(failure)?;
            if ids.len() >= self.maximum || ids.contains(&id) {
                return Err(failure());
            }
            ids.try_reserve(1).map_err(|_| failure())?;
            ids.push(id);
        }
        Ok(field)
    }
    fn read_list_begin(&mut self) -> thrift::Result<TListIdentifier> {
        self.typed(TType::List)?;
        let identifier = self.inner.read_list_begin()?;
        self.wire_type = Some(identifier.element_type);
        self.container(identifier.size, 1)?;
        Ok(identifier)
    }
    fn read_set_begin(&mut self) -> thrift::Result<TSetIdentifier> {
        self.typed(TType::Set)?;
        let identifier = self.inner.read_set_begin()?;
        self.wire_type = Some(identifier.element_type);
        self.container(identifier.size, 1)?;
        Ok(identifier)
    }
    fn read_map_begin(&mut self) -> thrift::Result<TMapIdentifier> {
        self.typed(TType::Map)?;
        let identifier = self.inner.read_map_begin()?;
        self.wire_type = None;
        self.container(identifier.size, 2)?;
        if identifier.size != 0
            && (identifier.key_type.is_none() || identifier.value_type.is_none())
        {
            return Err(failure());
        }
        Ok(identifier)
    }
    fn read_list<P: ReadThrift>(&mut self) -> thrift::Result<Vec<P>> {
        let identifier = self.read_list_begin()?;
        let count = usize::try_from(identifier.size).map_err(|_| failure())?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| failure())?;
        for _ in 0..count {
            self.wire_type = Some(identifier.element_type);
            values.push(P::read_from_in_protocol(self)?);
        }
        self.read_list_end()?;
        Ok(values)
    }
    // Default skip intentionally dispatches through this wrapper's container checks.
}
pub(crate) fn footer(
    bytes: &[u8],
    limits: LocalGraphLimits,
) -> Result<polars_parquet_format::FileMetaData> {
    let mut cursor = Cursor::new(bytes);
    let mut guard = Guard::new(&mut cursor, limits);
    let metadata = polars_parquet_format::FileMetaData::read_from_in_protocol(&mut guard)
        .map_err(|_| DataReadError::InvalidParquet("footer protocol"))?;
    drop(guard);
    if cursor.position() != bytes.len() as u64 {
        return Err(DataReadError::InvalidParquet("footer trailing bytes"));
    }
    Ok(metadata)
}
pub(crate) fn header(
    bytes: &[u8],
    limits: LocalGraphLimits,
) -> Result<(polars_parquet_format::PageHeader, usize)> {
    let mut cursor = Cursor::new(bytes);
    let metadata = polars_parquet_format::PageHeader::read_from_in_protocol(&mut Guard::new(
        (&mut cursor).take(limits.max_header_bytes as u64),
        limits,
    ))
    .map_err(|_| DataReadError::InvalidParquet("page protocol"))?;
    Ok((metadata, cursor.position() as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars_parquet_format::thrift::protocol::{
        TCompactOutputProtocol, TListIdentifier, TMapIdentifier, TOutputProtocol, TSetIdentifier,
    };
    fn list(count: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut writer = TCompactOutputProtocol::new(&mut bytes);
        writer
            .write_list_begin(&TListIdentifier::new(TType::I64, count))
            .unwrap();
        for index in 0..count.min(8) {
            writer.write_i64(index as i64).unwrap();
        }
        writer.write_list_end().unwrap();
        bytes
    }
    #[test]
    fn exact_list_limit_and_plus_one_fail_before_first_element() {
        let mut limits = crate::tests::policy();
        limits.max_list_entries = 3;
        let bytes = list(3);
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut protocol = Guard::new(&mut cursor, limits);
        assert_eq!(protocol.read_list::<i64>().unwrap(), [0, 1, 2]);
        let bytes = list(4);
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut protocol = Guard::new(&mut cursor, limits);
        assert!(protocol.read_list::<i64>().is_err());
        drop(protocol);
        assert_eq!(cursor.position(), 1);
    }
    #[test]
    fn aggregate_and_unknown_skip_container_limits_cannot_bypass_wrapper() {
        let mut limits = crate::tests::policy();
        limits.max_list_entries = 4;
        limits.max_protocol_entries = 3;
        let bytes = list(4);
        let mut cursor = Cursor::new(bytes.as_slice());
        assert!(Guard::new(&mut cursor, limits).skip(TType::List).is_err());
        assert_eq!(cursor.position(), 1);
        let bytes = [list(2), list(2)].concat();
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut guard = Guard::new(&mut cursor, limits);
        guard.read_list::<i64>().unwrap();
        assert!(guard.read_list::<i64>().is_err());
    }
    #[test]
    fn map_pairs_charge_twice_and_set_entries_are_bounded() {
        let mut limits = crate::tests::policy();
        limits.max_list_entries = 2;
        limits.max_protocol_entries = 3;
        let mut bytes = Vec::new();
        TCompactOutputProtocol::new(&mut bytes)
            .write_map_begin(&TMapIdentifier::new(Some(TType::I64), Some(TType::I64), 2))
            .unwrap();
        assert!(
            Guard::new(Cursor::new(&bytes), limits)
                .read_map_begin()
                .is_err()
        );
        let mut bytes = Vec::new();
        TCompactOutputProtocol::new(&mut bytes)
            .write_set_begin(&TSetIdentifier::new(TType::I64, 3))
            .unwrap();
        assert!(
            Guard::new(Cursor::new(&bytes), limits)
                .read_set_begin()
                .is_err()
        );
    }
    #[test]
    fn negative_signed_wire_container_length_is_denied_as_large_u32() {
        let bytes = list(u32::MAX);
        let limits = crate::tests::policy();
        let mut cursor = Cursor::new(bytes.as_slice());
        assert!(Guard::new(&mut cursor, limits).read_list::<i64>().is_err());
    }
    #[test]
    fn duplicate_struct_fields_and_scalar_type_mismatch_are_rejected() {
        use polars_parquet_format::thrift::protocol::{TFieldIdentifier, TStructIdentifier};
        for wrong_type in [false, true] {
            let mut bytes = Vec::new();
            let mut writer = TCompactOutputProtocol::new(&mut bytes);
            writer
                .write_struct_begin(&TStructIdentifier::new("metadata"))
                .unwrap();
            writer
                .write_field_begin(&TFieldIdentifier::new(
                    "version",
                    if wrong_type { TType::I64 } else { TType::I32 },
                    1,
                ))
                .unwrap();
            if wrong_type {
                writer.write_i64(1).unwrap();
            } else {
                writer.write_i32(1).unwrap();
                writer.write_field_end().unwrap();
                writer
                    .write_field_begin(&TFieldIdentifier::new("version", TType::I32, 1))
                    .unwrap();
                writer.write_i32(1).unwrap();
            }
            writer.write_field_end().unwrap();
            writer.write_field_stop().unwrap();
            writer.write_struct_end().unwrap();
            let mut guard = Guard::new(Cursor::new(&bytes), crate::tests::policy());
            guard.read_struct_begin().unwrap();
            guard.read_field_begin().unwrap();
            if wrong_type {
                assert!(guard.read_i32().is_err());
            } else {
                assert_eq!(guard.read_i32().unwrap(), 1);
                guard.read_field_end().unwrap();
                assert!(guard.read_field_begin().is_err());
            }
        }
    }
}
