use std::fmt;

/// Error type for `FullName` parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum FullNameError {
    /// The input contained an invalid number of parts (more than one '.').
    InvalidFormat,
    /// A part (namespace or name) contained an invalid character.
    /// Character codes must be printable ASCII (0x20 to 0x7E) and not '.' (0x2E).
    InvalidCharacter(u8),
    /// A part (namespace or name) was empty.
    EmptyPart,
}

impl fmt::Display for FullNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FullNameError::InvalidFormat => write!(
                f,
                "Invalid full name format: expected 'namespace.component' or 'component'"
            ),
            FullNameError::InvalidCharacter(c) => write!(
                f,
                "Invalid character with byte value 0x{:02X} found in name part",
                c
            ),
            FullNameError::EmptyPart => write!(f, "Namespace or component name cannot be empty"),
        }
    }
}

impl std::error::Error for FullNameError {}

/// Represents the full name of a Component with its namespace and name.
///
/// According to the LECO specification, a full name is composed of a namespace and
/// a component name, separated by a dot ('.', 0x2E).
/// - If no dot is present, the name is treated as a component name with an empty namespace.
/// - Component names and namespaces must consist only of printable ASCII characters
///   (byte values 0x20 to 0x7E) and must not contain the '.' character.
///
/// This struct holds borrowed references to the underlying data.
///
/// # Examples
///
/// ```
/// use ruleco_core::full_name::{FullName, FullNameError};
///
/// // Parsing a full name with namespace
/// let data = b"namespace_1.name_A";
/// let full_name = FullName::from_slice(data).unwrap();
/// assert_eq!(full_name.namespace(), b"namespace_1");
/// assert_eq!(full_name.name(), b"name_A");
///
/// // Parsing a component name without namespace
/// let data = b"name_B";
/// let full_name = FullName::from_slice(data).unwrap();
/// assert_eq!(full_name.namespace(), b"");
/// assert_eq!(full_name.name(), b"name_B");
///
/// // Handling errors
/// let invalid_data = b"ns.na.me"; // Too many dots
/// assert_eq!(FullName::from_slice(invalid_data), Err(FullNameError::InvalidFormat));
///
/// let invalid_char_data = b"ns.name\x01"; // Invalid character
/// assert!(matches!(FullName::from_slice(invalid_char_data), Err(FullNameError::InvalidCharacter(0x01))));
/// ```
#[derive(PartialEq, Debug)]
pub struct FullName<'a> {
    namespace: &'a [u8],
    name: &'a [u8],
}

impl<'a> FullName<'a> {
    /// Creates a `FullName` from a byte slice.
    ///
    /// Validates that the input conforms to the LECO naming rules.
    ///
    /// # Arguments
    ///
    /// * `slice` - A byte slice representing the full name.
    ///
    /// # Returns
    ///
    /// * `Ok(FullName)` - If the slice represents a valid full name.
    /// * `Err(FullNameError)` - If the slice is invalid.
    pub fn from_slice(slice: &'a [u8]) -> Result<Self, FullNameError> {
        // 46 is value of ASCII "."
        let parts: Vec<&[u8]> = slice.split(|e| *e == 46u8).collect();

        let full_name = match parts.len() {
            1 => {
                let name = parts[0];
                if name.is_empty() {
                    return Err(FullNameError::EmptyPart);
                }
                validate_part(name)?;
                Self {
                    namespace: &[],
                    name,
                }
            }
            2 => {
                let namespace = parts[0];
                let name = parts[1];

                if name.is_empty() {
                    return Err(FullNameError::EmptyPart);
                }

                validate_part(namespace)?;
                validate_part(name)?;

                Self { namespace, name }
            }
            _ => {
                // More than one dot found
                return Err(FullNameError::InvalidFormat);
            }
        };

        Ok(full_name)
    }

    /// Gets the namespace part of the full name.
    pub fn namespace(&self) -> &'a [u8] {
        self.namespace
    }

    /// Gets the component name part of the full name.
    pub fn name(&self) -> &'a [u8] {
        self.name
    }

    /// Checks if this full name has a namespace.
    pub fn has_namespace(&self) -> bool {
        !self.namespace.is_empty()
    }

    // Note: If we add OwnedFullName later, we could add:
    // pub fn to_owned(&self) -> OwnedFullName { ... }

    /// Converts the FullName into a string in the format "namespace"."name"
    pub fn to_string(&self) -> String {
        let namespace_str = std::str::from_utf8(self.namespace).unwrap_or("");
        let name_str = std::str::from_utf8(self.name).unwrap_or("");

        if self.has_namespace() {
            format!("{}.{}", namespace_str, name_str)
        } else {
            name_str.to_string()
        }
    }
}

/// Validates that a name part (namespace or component name) conforms to LECO rules.
/// Printable ASCII (0x20 to 0x7E) and not '.' (0x2E).
fn validate_part(part: &[u8]) -> Result<(), FullNameError> {
    for &byte in part {
        if !(0x20..=0x7E).contains(&byte) || byte == 0x2E {
            return Err(FullNameError::InvalidCharacter(byte));
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{FullName, FullNameError};

    #[test]
    fn test_full_name_with_namespace() {
        let data = b"namespace_1.name_A";
        let full_name = FullName::from_slice(data).unwrap();
        assert_eq!(full_name.namespace(), b"namespace_1");
        assert_eq!(full_name.name(), b"name_A");
        assert!(full_name.has_namespace());
    }

    #[test]
    fn test_full_name_without_namespace() {
        let data = b"name_B";
        let full_name = FullName::from_slice(data).unwrap();
        assert_eq!(full_name.namespace(), b"");
        assert_eq!(full_name.name(), b"name_B");
        assert!(!full_name.has_namespace());
    }

    #[test]
    fn test_full_name_empty_name() {
        let data = b"";
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::EmptyPart));
    }

    #[test]
    fn test_full_name_empty_name_part() {
        let data = b"namespace.";
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::EmptyPart));

        let data = b".name";
        let result = FullName::from_slice(data);
        let full_name = result.unwrap();
        assert_eq!(full_name.namespace(), b"");
        assert_eq!(full_name.name(), b"name");
        // If strict leading dot rejection is desired, validation logic would change.
    }

    #[test]
    fn test_full_name_invalid_format_too_many_dots() {
        let data = b"ns.na.me";
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::InvalidFormat));
    }

    #[test]
    fn test_full_name_invalid_character_non_printable_low() {
        let data = b"ns.name\x01"; // SOH character
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::InvalidCharacter(0x01)));
    }

    #[test]
    fn test_full_name_invalid_character_non_printable_high() {
        let data = b"ns.name\x7F"; // DEL character
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::InvalidCharacter(0x7F)));
    }

    #[test]
    fn test_full_name_invalid_character_dot_in_part() {
        let data = b"ns.nam\x2Ee";
        let result2 = FullName::from_slice(data);
        assert_eq!(result2, Err(FullNameError::InvalidCharacter(0x2E))); // 0x2E is '.'
    }

    #[test]
    fn test_full_name_valid_characters_edge_cases() {
        // Test space (0x20) and tilde (0x7E)
        let data = b"ns.with space.component~name";
        let full_name = FullName::from_slice(data).unwrap();
        assert_eq!(full_name.namespace(), b"ns.with space");
        assert_eq!(full_name.name(), b"component~name");
    }
}
