//! # RuLECO
//!
//! `ruleco` is a Rust implementation of the Laboratory Control Protocol (LECO)

const VERSION: u8 = 0; // LECO protocol version

pub mod core {
    use uuid::Uuid;

    /// Create a new conversation id
    pub fn create_conversation_id() -> [u8; 16] {
        //should be a UUIDv7
        //b"conversation_id;"
        let uuid = Uuid::now_v7();
        /*let cid: [u8; 16] = [
            99, 111, 110, 118, 101, 114, 115, 97, 116, 105, 111, 110, 95, 105, 100, 59,
        ];*/
        return uuid.into_bytes();
    }

    /// Different types of content
    pub enum ContentTypes {
        Frames(Vec<Vec<u8>>),
        Frame(Vec<u8>),
        Null,
    }

    /// Describe the full name of a Component with its namespace and name
    ///
    /// # Examples
    ///
    /// ```
    /// use ruleco::core::FullName;
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
    /// use ruleco::core::FullName;
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
        use crate::core::FullName;

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
}

pub mod control_protocol;

pub mod data_protocol;
