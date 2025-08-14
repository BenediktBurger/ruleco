/// Describe the full name of a Component with its namespace and name
///
/// # Examples
///
/// ```
/// use ruleco_core::full_name::FullName;
/// let name_vector = b"namespace_1.name_A".to_vec();
/// let full_name = FullName::from_vec(&name_vector).unwrap();
/// assert_eq!(
///     full_name,
///     FullName {
///         namespace: b"namespace_1",
///         name: b"name_A",
/// });
/// ```
/// ```
/// use ruleco_core::full_name::FullName;
/// let full_name = FullName::from_slice(b"namespace_1.name_A").unwrap();
/// assert_eq!(full_name,
///     FullName {
///         namespace: b"namespace_1",
///         name: b"name_A",
/// });
/// ```
#[derive(PartialEq, Debug)]
pub struct FullName<'a> {
    pub namespace: &'a [u8],
    pub name: &'a [u8],
}

impl<'a> FullName<'a> {
    fn from_split(split: Vec<&'a [u8]>) -> Result<Self, String> {
        match split.len() {
            1 => Ok(Self {
                namespace: &[],
                name: split[0],
            }),
            2 => Ok(Self {
                namespace: split[0],
                name: split[1],
            }),
            x => Err(format!("Invalid number {x} of elements in name found.")),
        }
    }
    pub fn from_vec(vec: &'a Vec<u8>) -> Result<Self, String> {
        // 46 is value of ASCII "."
        let parts: Vec<&[u8]> = vec.split(|e| *e == 46u8).collect();
        Self::from_split(parts)
    }
    pub fn from_slice(slice: &'a [u8]) -> Result<Self, String> {
        let parts: Vec<&[u8]> = slice.split(|e| *e == 46u8).collect();
        Self::from_split(parts)
    }
}

#[cfg(test)]
mod test {
    use super::FullName;

    #[test]
    fn test_full_name() {
        let full_name = b"abc.def".to_vec();
        assert_eq!(
            FullName::from_vec(&full_name).unwrap(),
            FullName {
                namespace: b"abc",
                name: b"def",
            }
        )
    }
    #[test]
    fn test_full_name_without_namespace() {
        let full_name = b"def".to_vec();
        assert_eq!(
            FullName::from_vec(&full_name).unwrap(),
            FullName {
                namespace: b"",
                name: b"def",
            }
        )
    }
}
