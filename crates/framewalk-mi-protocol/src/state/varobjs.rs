//! Variable object registry.

use std::collections::BTreeMap;

use framewalk_mi_codec::{ListValue, Value};

use crate::results_view::{get_bool, get_str, get_u32};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarObjName(String);

impl VarObjName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarObj {
    pub name: VarObjName,
    pub expression: Option<String>,
    pub type_name: Option<String>,
    pub value: Option<String>,
    pub numchild: Option<u32>,
    pub in_scope: Option<bool>,
}

impl VarObj {
    fn new(name: VarObjName) -> Self {
        Self {
            name,
            expression: None,
            type_name: None,
            value: None,
            numchild: None,
            in_scope: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct VarObjRegistry {
    by_name: BTreeMap<VarObjName, VarObj>,
}

impl VarObjRegistry {
    pub fn iter(&self) -> impl Iterator<Item = (&VarObjName, &VarObj)> {
        self.by_name.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    #[must_use]
    pub fn get(&self, name: &VarObjName) -> Option<&VarObj> {
        self.by_name.get(name)
    }

    pub(crate) fn on_var_create(
        &mut self,
        results: &[(String, Value)],
        expression: Option<String>,
    ) {
        let Some(name) = get_str(results, "name") else {
            return;
        };
        let key = VarObjName::new(name);
        let mut varobj = VarObj::new(key.clone());
        varobj.expression = expression;
        varobj.type_name = get_str(results, "type").map(str::to_owned);
        varobj.value = get_str(results, "value").map(str::to_owned);
        varobj.numchild = get_u32(results, "numchild");
        self.by_name.insert(key, varobj);
    }

    pub(crate) fn on_var_update(&mut self, results: &[(String, Value)]) {
        let Some(Value::List(list)) =
            results
                .iter()
                .find_map(|(k, v)| if k == "changelist" { Some(v) } else { None })
        else {
            return;
        };
        let entries: &[Value] = match list {
            ListValue::Values(vs) => vs.as_slice(),
            ListValue::Results(_) | ListValue::Empty => return,
        };
        for entry in entries {
            let Value::Tuple(pairs) = entry else { continue };
            let Some(name) = get_str(pairs, "name") else {
                continue;
            };
            let key = VarObjName::new(name);
            let varobj = self
                .by_name
                .entry(key.clone())
                .or_insert_with(|| VarObj::new(key));
            if let Some(value) = get_str(pairs, "value") {
                varobj.value = Some(value.to_owned());
            }
            if let Some(in_scope) = get_bool(pairs, "in_scope") {
                varobj.in_scope = Some(in_scope);
            }
            if let Some(type_name) = get_str(pairs, "new_type") {
                varobj.type_name = Some(type_name.to_owned());
            }
        }
    }

    pub(crate) fn on_var_delete(&mut self, name: &str) {
        self.by_name.remove(&VarObjName::new(name));
    }
}
