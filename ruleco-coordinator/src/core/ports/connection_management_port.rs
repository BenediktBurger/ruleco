use anyhow::Result;

/// Interface for connection management operations
pub trait ConnectionManagementPort {
    fn listen_for_components(&mut self, address: &str) -> Result<()>;

    fn connect_to_coordinator(
        &mut self,
        address: &str,
    ) -> Result<Vec<u8>>;

    fn disconnect_from_coordinator(
        &mut self,
        identity: &[u8],
    ) -> Result<()>;
}
