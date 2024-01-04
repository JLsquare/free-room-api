# free-room-api

## Quick Start Guide

### Prerequisites
- Rust environment

### Installation
1. Clone the repository.
2. Run `cargo build` to install dependencies.

### Running the Server
- Execute `cargo run` to start the server.
- Server listens on `127.0.0.1:8080`.

### API Endpoints
1. **All Rooms**: `/api/all` (GET)
    - Returns all room availability slots.

2. **Room Availability by Hour Offset**: `/api/lite/{hour_offset}` (GET)
    - hour_offset: use '0', else it's for testing purposes.
    - Returns room availability for each rooms with :
      - `name`: name of the room.
      - `status`: if the room is available.
      - `duration`: how long the room is available for or in how long the room will be available for.

### Note
- Room data is auto-updated periodically.
- Check source code for more details.