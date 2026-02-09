/// Interface for connection management operations
pub trait ConnectionManagementPort {
    /// Start listening for component connections
    ///
    /// Binds to an address to accept connections from local components.
    /// The specific transport mechanism is abstracted away.
    ///
    /// # Parameters
    /// * `address` - Address to listen on (e.g., "tcp://*:12300")
    fn listen_for_components(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>>;

    /// Connect to a remote coordinator
    ///
    /// Establishes a connection to another coordinator's listening address.
    ///
    /// # Parameters
    /// * `address` - Address of the remote coordinator's listening endpoint
    ///
    /// # Returns
    /// The connection identity for this remote coordinator
    fn connect_to_coordinator(
        &mut self,
        address: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;

    /// Disconnect from a remote coordinator
    ///
    /// Closes the connection to a remote coordinator.
    ///
    /// # Parameters
    /// * `identity` - The connection identity of the remote coordinator to disconnect
    fn disconnect_from_coordinator(
        &mut self,
        identity: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
}
