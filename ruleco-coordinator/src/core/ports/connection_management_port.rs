/// Interface for connection management operations
pub trait ConnectionManagementPort {
    /// Create a connection to a remote coordinator
    ///
    /// # Parameters
    /// * `remote_address` - The address of the remote coordinator
    /// # Returns
    /// The identity to use for the dealer connection
    fn connect_to_coordinator(
        &mut self,
        remote_address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;

    /// Disconnect from a remote coordinator
    ///
    /// # Parameters
    /// * `dealer_identity` - The identity of the dealer connection to remove
    fn disconnect_from_coordinator(
        &mut self,
        dealer_identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Bind the router socket to an address
    ///
    /// # Parameters
    /// * `address` - The address to bind the router socket to
    fn bind_router(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>>;
}
