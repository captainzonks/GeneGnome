// ==============================================================================
// middleware/mod.rs - API Gateway Middleware Modules
// ==============================================================================
// Description: Authentication and request processing middleware
// Author: Matt Barham
// Created: 2026-01-11
// Modified: 2026-01-11
// Version: 1.0.0
// ==============================================================================

pub mod auth;

pub use auth::AuthUser;
