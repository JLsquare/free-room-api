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
    - Returns all room availability.

2. **Room Availability by Hour Offset**: `/api/lite/{hour_offset}` (GET)
    - `{hour_offset}` is optional; defaults to current time.

### Note
- Room data is auto-updated periodically.
- Check source code for more details.