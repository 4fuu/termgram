use std::collections::HashMap;

use compact_str::CompactString;
use mlua::{BString, IntoLua, Lua, Value};

use crate::parser::StateOsc5522;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardEvent {
	Read { id: String, primary: bool, pw: String, data: ClipboardData },
	ReadError { id: String, code: CompactString },
	WriteSuccess,
	WriteError(CompactString),
}

impl ClipboardEvent {
	pub(crate) fn r#type(&self) -> &'static str {
		match self {
			Self::Read { .. } => "read",
			Self::ReadError { .. } | Self::WriteError(_) => "error",
			Self::WriteSuccess => "success",
		}
	}

	pub(crate) fn primary(&self) -> Option<bool> {
		match self {
			Self::Read { primary, .. } => Some(*primary),
			_ => None,
		}
	}

	pub(crate) fn pw(&mut self) -> Option<&mut String> {
		match self {
			Self::Read { pw, .. } => Some(pw),
			_ => None,
		}
	}

	pub(crate) fn data(&mut self) -> Option<&mut ClipboardData> {
		match self {
			Self::Read { data, .. } => Some(data),
			_ => None,
		}
	}

	pub(crate) fn is_write(&self) -> bool { matches!(self, Self::WriteSuccess | Self::WriteError(_)) }

	pub(crate) fn from_state(s: StateOsc5522) -> Option<Self> {
		Some(match s {
			StateOsc5522 { write: false, status, .. } if status == "DONE" => Self::Read {
				id:      s.id,
				primary: s.primary,
				pw:      s.pw,
				data:    ClipboardData(s.mimes.into_iter().zip(s.payload).collect()),
			},
			StateOsc5522 { write: false, status, .. } if !status.is_empty() => Self::ReadError { id: s.id, code: status },
			StateOsc5522 { write: true, status, .. } if status == "DONE" => Self::WriteSuccess,
			StateOsc5522 { write: true, status, .. } if !status.is_empty() => Self::WriteError(status),
			_ => return None,
		})
	}
}

// --- ClipboardData
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClipboardData(HashMap<String, Vec<u8>>);

impl ClipboardData {
	pub fn get(&self, mime: &str) -> Option<&[u8]> { self.0.get(mime).map(Vec::as_slice) }
	pub fn into_inner(self) -> HashMap<String, Vec<u8>> { self.0 }
}

impl FromIterator<(String, Vec<u8>)> for ClipboardData {
	fn from_iter<T: IntoIterator<Item = (String, Vec<u8>)>>(iter: T) -> Self {
		Self(iter.into_iter().collect())
	}
}

impl IntoLua for ClipboardData {
	fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
		lua
			.create_table_from(self.0.into_iter().map(|(mime, payload)| (mime, BString::new(payload))))?
			.into_lua(lua)
	}
}
