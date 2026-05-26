use std::fmt;

/// Error type for `FullName` parsing.
#[derive(Debug, PartialEq, Eq, Clone)]
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
                "Invalid character with byte value 0x{c:02X} found in name part"
            ),
            FullNameError::EmptyPart => write!(f, "Namespace or component name cannot be empty"),
        }
    }
}

impl std::error::Error for FullNameError {}

/// Represent the full name of a Component with its namespace and name.
///
/// According to the LECO specification, a full name is composed of a namespace and
/// a component name, separated by a dot ('.', 0x2E).
/// - If no dot is present, the name is treated as a component name with an empty namespace.
/// - Component names and namespaces must consist only of printable ASCII characters
///   (byte values 0x20 to 0x7E) and must not contain the '.' character.
///
/// This struct owns its data, making it suitable for long-term storage.
///
/// # Examples
///
/// ```
/// use ruleco_core::full_name::{FullName, FullNameError};
///
/// // Parsing a full name with namespace
/// let full_name = FullName::from_slice(b"namespace_1.name_A").unwrap();
/// assert_eq!(full_name.namespace(), b"namespace_1");
/// assert_eq!(full_name.name(), b"name_A");
///
/// // Parsing a component name without namespace
/// let full_name = FullName::from_slice(b"name_B").unwrap();
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
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct FullName {
    namespace: Vec<u8>,
    name: Vec<u8>,
}

impl FullName {
    pub fn new(namespace: Vec<u8>, name: Vec<u8>) -> Self {
        Self { namespace, name }
    }

    /// Create a `FullName` from a byte slice.
    ///
    /// Validate that the input conforms to the LECO naming rules.
    ///
    /// # Arguments
    ///
    /// * `slice` - A byte slice representing the full name.
    ///
    /// # Returns
    ///
    /// * `Ok(FullName)` - If the slice represents a valid full name.
    /// * `Err(FullNameError)` - If the slice is invalid.
    pub fn from_slice(slice: &[u8]) -> Result<Self, FullNameError> {
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
                    namespace: vec![],
                    name: name.to_vec(),
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

                Self {
                    namespace: namespace.to_vec(),
                    name: name.to_vec(),
                }
            }
            _ => {
                // More than one dot found
                return Err(FullNameError::InvalidFormat);
            }
        };

        Ok(full_name)
    }

    /// Get the namespace part of the full name.
    pub fn namespace(&self) -> &[u8] {
        &self.namespace
    }

    /// Get the component name part of the full name.
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Check if this full name has a namespace.
    pub fn has_namespace(&self) -> bool {
        !self.namespace.is_empty()
    }

    /// Convert the FullName into a string in the format "namespace"."name"
    pub fn to_string(&self) -> String {
        let namespace_str = std::str::from_utf8(&self.namespace).unwrap_or("");
        let name_str = std::str::from_utf8(&self.name).unwrap_or("");

        if self.has_namespace() {
            format!("{namespace_str}.{name_str}")
        } else {
            name_str.to_string()
        }
    }

    /// Create a `FullName` from a string slice.
    ///
    /// This is a convenience method, primarily intended for testing,
    /// equivalent to `FullName::from_slice(s.as_bytes())`.
    ///
    /// # Arguments
    ///
    /// * `s` - A string slice representing the full name.
    ///
    /// # Returns
    ///
    /// * `Ok(FullName)` - If the string represents a valid full name.
    /// * `Err(FullNameError)` - If the string is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco_core::full_name::{FullName, FullNameError};
    ///
    /// let full_name = FullName::from_str("namespace.component").unwrap();
    /// assert_eq!(full_name.namespace(), b"namespace");
    /// assert_eq!(full_name.name(), b"component");
    ///
    /// assert_eq!(FullName::from_str("invalid..format"), Err(FullNameError::InvalidFormat));
    /// ```
    pub fn from_str(s: &str) -> Result<Self, FullNameError> {
        Self::from_slice(s.as_bytes())
    }

    /// Convert the `FullName` into a `Vec<u8>` representation.
    ///
    /// This is the inverse operation of `from_slice`.
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` containing the full name in the format "namespace.name" or "name".
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco_core::full_name::FullName;
    ///
    /// let full_name = FullName::from_str("namespace.component").unwrap();
    /// let bytes = full_name.to_vec();
    /// assert_eq!(bytes, b"namespace.component");
    ///
    /// let full_name = FullName::from_str("component").unwrap();
    /// let bytes = full_name.to_vec();
    /// assert_eq!(bytes, b"component");
    /// ```
    pub fn to_vec(&self) -> Vec<u8> {
        if self.has_namespace() {
            let mut result = Vec::with_capacity(self.namespace.len() + 1 + self.name.len());
            result.extend_from_slice(&self.namespace);
            result.push(b'.');
            result.extend_from_slice(&self.name);
            result
        } else {
            self.name.clone()
        }
    }

    /// Create a `FullName` from two string slices.
    ///
    /// This is a convenience method that directly constructs a FullName
    /// from namespace and name strings without the overhead of formatting
    /// and parsing.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace string (can be empty)
    /// * `name` - The component name string
    ///
    /// # Returns
    ///
    /// * `Ok(FullName)` - If both strings are valid
    /// * `Err(FullNameError)` - If either string contains invalid characters or name is empty
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco_core::full_name::{FullName, FullNameError};
    ///
    /// let full_name = FullName::from_strings("namespace", "component").unwrap();
    /// assert_eq!(full_name.namespace(), b"namespace");
    /// assert_eq!(full_name.name(), b"component");
    ///
    /// let full_name = FullName::from_strings("", "component").unwrap();
    /// assert_eq!(full_name.namespace(), b"");
    /// assert_eq!(full_name.name(), b"component");
    ///
    /// assert_eq!(FullName::from_strings("ns", ""), Err(FullNameError::EmptyPart));
    /// ```
    pub fn from_strings(namespace: &str, name: &str) -> Result<Self, FullNameError> {
        validate_part(namespace.as_bytes())?;
        validate_part(name.as_bytes())?;

        if name.is_empty() {
            return Err(FullNameError::EmptyPart);
        }

        Ok(Self {
            namespace: namespace.as_bytes().to_vec(),
            name: name.as_bytes().to_vec(),
        })
    }
}

/// Validate that a name part (namespace or component name) conforms to LECO rules.
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
        // This test is checking that a dot in a part (namespace or name) is rejected.
        // However, our current implementation splits on dots first, so this will actually
        // result in InvalidFormat if there are too many dots, not InvalidCharacter.
        // Let's adjust the test to match the actual behavior.
        let data = b"ns.nam\x2Ee"; // This has three parts, so should be InvalidFormat
        let result = FullName::from_slice(data);
        assert_eq!(result, Err(FullNameError::InvalidFormat));

        // To test InvalidCharacter for a dot, we'd need a case where a dot is part of a single part.
        // But since we split on dots first, this isn't possible with the current implementation.
        // The existing test expectation was incorrect for the current implementation.
    }

    #[test]
    fn test_full_name_valid_characters_edge_cases() {
        // Test space (0x20) and tilde (0x7E)
        let data2 = b"ns with space.component~name";
        let full_name2 = FullName::from_slice(data2).unwrap();
        assert_eq!(full_name2.namespace(), b"ns with space");
        assert_eq!(full_name2.name(), b"component~name");
    }

    // Tests for from_str method
    #[test]
    fn test_from_str_with_namespace() {
        let input = "my_ns.my_component";
        let full_name = FullName::from_str(input).expect("Parsing failed");
        assert_eq!(full_name.namespace(), b"my_ns");
        assert_eq!(full_name.name(), b"my_component");
        assert!(full_name.has_namespace());
    }

    #[test]
    fn test_from_str_without_namespace() {
        let input = "standalone_component";
        let full_name = FullName::from_str(input).expect("Parsing failed");
        assert_eq!(full_name.namespace(), b"");
        assert_eq!(full_name.name(), b"standalone_component");
        assert!(!full_name.has_namespace());
    }

    #[test]
    fn test_from_str_empty_input() {
        let input = "";
        let result = FullName::from_str(input);
        assert_eq!(result, Err(FullNameError::EmptyPart));
    }

    #[test]
    fn test_from_str_invalid_format_too_many_dots() {
        let input = "ns.comp.onent";
        let result = FullName::from_str(input);
        assert_eq!(result, Err(FullNameError::InvalidFormat));
    }

    #[test]
    fn test_from_str_invalid_character() {
        let input = "ns.comp\x01onent";
        let result = FullName::from_str(input);
        assert_eq!(result, Err(FullNameError::InvalidCharacter(0x01)));
    }

    #[test]
    fn test_from_str_using_from_str_method_directly() {
        // Test the direct method call as well
        let full_name = FullName::from_str("direct.call").unwrap();
        assert_eq!(full_name.namespace(), b"direct");
        assert_eq!(full_name.name(), b"call");
    }

    // Tests for to_vec method
    #[test]
    fn test_to_vec_with_namespace() {
        let full_name = FullName::from_str("namespace.component").unwrap();
        let bytes = full_name.to_vec();
        assert_eq!(bytes, b"namespace.component");
    }

    #[test]
    fn test_to_vec_without_namespace() {
        let full_name = FullName::from_str("component").unwrap();
        let bytes = full_name.to_vec();
        assert_eq!(bytes, b"component");
    }

    #[test]
    fn test_to_vec_roundtrip() {
        // Test that from_slice and to_vec are inverses
        let original = b"test.namespace";
        let full_name = FullName::from_slice(original).unwrap();
        let bytes = full_name.to_vec();
        assert_eq!(bytes, original);

        // And the other way around
        let full_name = FullName::from_str("another.test").unwrap();
        let bytes = full_name.to_vec();
        let full_name2 = FullName::from_slice(&bytes).unwrap();
        assert_eq!(full_name, full_name2);
    }

    // Tests for from_strings method
    #[test]
    fn test_from_strings_with_namespace() {
        let full_name = FullName::from_strings("namespace", "component").unwrap();
        assert_eq!(full_name.namespace(), b"namespace");
        assert_eq!(full_name.name(), b"component");
        assert!(full_name.has_namespace());
    }

    #[test]
    fn test_from_strings_without_namespace() {
        let full_name = FullName::from_strings("", "component").unwrap();
        assert_eq!(full_name.namespace(), b"");
        assert_eq!(full_name.name(), b"component");
        assert!(!full_name.has_namespace());
    }

    #[test]
    fn test_from_strings_empty_name() {
        let result = FullName::from_strings("namespace", "");
        assert_eq!(result, Err(FullNameError::EmptyPart));
    }

    #[test]
    fn test_from_strings_invalid_character_in_namespace() {
        let result = FullName::from_strings("ns\x01", "component");
        assert_eq!(result, Err(FullNameError::InvalidCharacter(0x01)));
    }

    #[test]
    fn test_from_strings_invalid_character_in_name() {
        let result = FullName::from_strings("namespace", "comp\x01onent");
        assert_eq!(result, Err(FullNameError::InvalidCharacter(0x01)));
    }

    #[test]
    fn test_from_strings_valid_characters() {
        let full_name = FullName::from_strings("ns with space", "component~name").unwrap();
        assert_eq!(full_name.namespace(), b"ns with space");
        assert_eq!(full_name.name(), b"component~name");
    }

    #[test]
    fn test_from_strings_roundtrip_with_to_vec() {
        let full_name = FullName::from_strings("test_ns", "test_component").unwrap();
        let bytes = full_name.to_vec();
        assert_eq!(bytes, b"test_ns.test_component");
    }

    #[test]
    fn test_from_strings_roundtrip_without_namespace() {
        let full_name = FullName::from_strings("", "standalone").unwrap();
        let bytes = full_name.to_vec();
        assert_eq!(bytes, b"standalone");
    }

    #[test]
    fn test_from_strings_equivalence_with_from_str() {
        let from_strings = FullName::from_strings("namespace", "component").unwrap();
        let from_str = FullName::from_str("namespace.component").unwrap();
        assert_eq!(from_strings, from_str);
    }

    #[test]
    fn test_from_strings_equivalence_without_namespace() {
        let from_strings = FullName::from_strings("", "component").unwrap();
        let from_str = FullName::from_str("component").unwrap();
        assert_eq!(from_strings, from_str);
    }
}
