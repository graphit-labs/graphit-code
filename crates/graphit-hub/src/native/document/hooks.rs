//! Tuple edits for native flat/grouped JSON commands and Kimi TOML rules.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookPosition {
    group: Option<usize>,
    tuple: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookMatch {
    Missing,
    Unique(HookPosition),
    Ambiguous,
}
impl HookMatch {
    fn add(&mut self, position: HookPosition) {
        *self = match self {
            Self::Missing => Self::Unique(position),
            _ => Self::Ambiguous,
        };
    }
}
fn json_matches(
    value: &Ast<'_>,
    fields: &TupleFields,
    request: &RequestContext<'_>,
) -> Result<bool> {
    let Some(object) = value.as_object() else {
        return Ok(false);
    };
    for (key, expected) in fields {
        check_request(request)?;
        match (object.get(key), expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) if json_equal(&actual.value, expected, request)? => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn toml_matches(
    table: &dyn toml_edit::TableLike,
    fields: &TupleFields,
    request: &RequestContext<'_>,
) -> Result<bool> {
    for (key, expected) in fields {
        check_request(request)?;
        match (table.get(key), expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) => match toml_field(actual, request) {
                Ok(value) if value == *expected => {}
                Ok(_) | Err(DocumentError::WrongType) => return Ok(false),
                Err(error) => return Err(error),
            },
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn json_has_comments(
    node: &jsonc_parser::cst::CstNode,
    request: &RequestContext<'_>,
) -> Result<bool> {
    check_request(request)?;
    if node.is_comment() {
        return Ok(true);
    }
    for child in node.children() {
        if json_has_comments(&child, request)? {
            return Ok(true);
        }
    }
    Ok(false)
}
impl NativeDocument<'_, '_> {
    pub(crate) fn hook_match(
        &self,
        path: &[&str],
        group: Option<&TupleFields>,
        fields: &TupleFields,
    ) -> Result<HookMatch> {
        validate_path(path, self.limits)?;
        check_request(self.request)?;
        let mut matched = HookMatch::Missing;
        match &self.tree {
            Tree::Json(_) => {
                let text = self.copy_text()?;
                let parsed = jsonc_parser::parse_to_ast(
                    &text,
                    &Default::default(),
                    &json_options(self.format),
                )
                .map_err(|_| DocumentError::InvalidDocument)?;
                let mut value = parsed
                    .value
                    .as_ref()
                    .ok_or(DocumentError::InvalidDocument)?;
                for key in path {
                    let object = value.as_object().ok_or(DocumentError::WrongType)?;
                    let Some(prop) = object.get(key) else {
                        return Ok(matched);
                    };
                    value = &prop.value;
                }
                let array = value.as_array().ok_or(DocumentError::WrongType)?;
                for (outer, value) in array.elements.iter().enumerate() {
                    check_request(self.request)?;
                    if let Some(group) = group {
                        if !json_matches(value, group, self.request)? {
                            continue;
                        }
                        let handlers = value
                            .as_object()
                            .and_then(|v| v.get("hooks"))
                            .and_then(|p| p.value.as_array())
                            .ok_or(DocumentError::WrongType)?;
                        for (tuple, value) in handlers.elements.iter().enumerate() {
                            if json_matches(value, fields, self.request)? {
                                matched.add(HookPosition {
                                    group: Some(outer),
                                    tuple,
                                });
                            }
                        }
                    } else if json_matches(value, fields, self.request)? {
                        matched.add(HookPosition {
                            group: None,
                            tuple: outer,
                        });
                    }
                }
            }
            Tree::Toml(document) => {
                if group.is_some() {
                    return Err(DocumentError::WrongType);
                }
                let mut item = document.as_item();
                for key in path {
                    let table = item.as_table_like().ok_or(DocumentError::WrongType)?;
                    let Some(next) = table.get(key) else {
                        return Ok(matched);
                    };
                    item = next;
                }
                if let Some(array) = item.as_array_of_tables() {
                    for (tuple, table) in array.iter().enumerate() {
                        if toml_matches(table, fields, self.request)? {
                            matched.add(HookPosition { group: None, tuple });
                        }
                    }
                } else if let Some(array) = item.as_array() {
                    for (tuple, value) in array.iter().enumerate() {
                        let table = value.as_inline_table().ok_or(DocumentError::WrongType)?;
                        if toml_matches(table, fields, self.request)? {
                            matched.add(HookPosition { group: None, tuple });
                        }
                    }
                } else {
                    return Err(DocumentError::WrongType);
                }
            }
        }
        check_request(self.request)?;
        Ok(matched)
    }

    /// Positions are observed under the shared writer lock and never persisted.
    pub(crate) fn edit_hook(
        &mut self,
        path: &[&str],
        group: Option<&TupleFields>,
        position: Option<HookPosition>,
        tuple: &FieldValue,
        fields: &TupleFields,
        remove: bool,
    ) -> Result<()> {
        validate_path(path, self.limits)?;
        validate_field(tuple, 1, &mut Budget::new(self.limits, self.request))?;
        let text = self.copy_text()?;
        let mut candidate = parse_tree(self.format, &text, self.limits, self.request)?;
        match &mut candidate {
            Tree::Json(root) => {
                let outer = json_array(root, path, position.is_none() && !remove)?;
                if let Some(position) = position {
                    let (array, wrapper) = if let Some(index) = position.group {
                        let wrapper = outer
                            .elements()
                            .get(index)
                            .and_then(|v| v.as_object())
                            .ok_or(DocumentError::WrongType)?;
                        let array = wrapper
                            .get("hooks")
                            .and_then(|v| v.array_value())
                            .ok_or(DocumentError::WrongType)?;
                        (array, Some(wrapper))
                    } else {
                        (outer.clone(), None)
                    };
                    let node = array
                        .elements()
                        .get(position.tuple)
                        .cloned()
                        .ok_or(DocumentError::WrongType)?;
                    if remove {
                        node.remove();
                        // An owned action does not own foreign fields added to its wrapper.
                        if array.elements().is_empty()
                            && let Some(wrapper) = wrapper
                        {
                            let owned = group.ok_or(DocumentError::WrongType)?;
                            let mut has_comments = false;
                            for child in wrapper.children() {
                                if json_has_comments(&child, self.request)? {
                                    has_comments = true;
                                    break;
                                }
                            }
                            if !has_comments
                                && wrapper.properties().iter().all(|p| {
                                    p.decoded_name().is_some_and(|name| {
                                        name == "hooks" || owned.contains_key(&name)
                                    })
                                })
                            {
                                wrapper.remove();
                            }
                        }
                    } else {
                        let object = node.as_object().ok_or(DocumentError::WrongType)?;
                        for (key, value) in fields {
                            check_request(self.request)?;
                            match (object.get(key), value) {
                                (Some(prop), Some(value)) => prop.set_value(json_input(value)),
                                (None, Some(value)) => {
                                    object.append(key, json_input(value));
                                }
                                (Some(prop), None) => prop.remove(),
                                (None, None) => {}
                            }
                        }
                    }
                } else {
                    if remove {
                        return Err(DocumentError::WrongType);
                    }
                    if let Some(group) = group {
                        let mut wrapper: BTreeMap<String, FieldValue> = group
                            .iter()
                            .filter_map(|(key, value)| value.clone().map(|v| (key.clone(), v)))
                            .collect();
                        wrapper.insert("hooks".into(), FieldValue::Array(vec![tuple.clone()]));
                        outer.append(json_input(&FieldValue::Object(wrapper)));
                    } else {
                        outer.append(json_input(tuple));
                    }
                }
            }
            Tree::Toml(document) => {
                if group.is_some() {
                    return Err(DocumentError::WrongType);
                }
                let mut parent = document.as_item_mut();
                for key in &path[..path.len() - 1] {
                    let table = parent.as_table_like_mut().ok_or(DocumentError::WrongType)?;
                    if table.get(key).is_none() {
                        table.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
                    }
                    parent = table.get_mut(key).ok_or(DocumentError::WrongType)?;
                }
                let table = parent.as_table_like_mut().ok_or(DocumentError::WrongType)?;
                let key = path[path.len() - 1];
                if table.get(key).is_none() {
                    table.insert(
                        key,
                        toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
                    );
                }
                let item = table.get_mut(key).ok_or(DocumentError::WrongType)?;
                if let Some(array) = item.as_array_of_tables_mut() {
                    if let Some(position) = position {
                        if remove {
                            if position.tuple >= array.len() {
                                return Err(DocumentError::WrongType);
                            }
                            array.remove(position.tuple);
                        } else {
                            let table = array
                                .get_mut(position.tuple)
                                .ok_or(DocumentError::WrongType)?;
                            edit_toml_fields(table, fields)?;
                        }
                    } else {
                        let FieldValue::Object(values) = tuple else {
                            return Err(DocumentError::WrongType);
                        };
                        let mut table = toml_edit::Table::new();
                        for (key, value) in values {
                            table.insert(key, toml_edit::Item::Value(toml_input(value)));
                        }
                        array.push(table);
                    }
                } else if let Some(array) = item.as_array_mut() {
                    if let Some(position) = position {
                        if remove {
                            if position.tuple >= array.len() {
                                return Err(DocumentError::WrongType);
                            }
                            array.remove(position.tuple);
                        } else {
                            let table = array
                                .get_mut(position.tuple)
                                .and_then(|v| v.as_inline_table_mut())
                                .ok_or(DocumentError::WrongType)?;
                            edit_toml_fields(table, fields)?;
                        }
                    } else {
                        array.push(toml_input(tuple));
                    }
                } else {
                    return Err(DocumentError::WrongType);
                }
            }
        }
        let output = render_tree(&candidate, self.limits.output_bytes, self.request)?;
        self.commit_output(&output)
    }
}
fn edit_toml_fields(table: &mut dyn toml_edit::TableLike, fields: &TupleFields) -> Result<()> {
    for (key, value) in fields {
        match value {
            Some(value) => {
                let mut value = toml_input(value);
                if let Some(old) = table.get(key).and_then(|i| i.as_value()) {
                    *value.decor_mut() = old.decor().clone();
                }
                table.insert(key, toml_edit::Item::Value(value));
            }
            None => {
                table.remove(key);
            }
        }
    }
    Ok(())
}
