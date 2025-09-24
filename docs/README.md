# Documentation

This directory contains the documentation for `multifaustus`.

## Setup

Run the setup script to install required tools:

```bash
just bootstrap-docs
```

This will install:

- mdBook (documentation generator)
- mdbook-mermaid (for diagram rendering)

## Building Documentation

To build the documentation:

```bash
just docs-build
```

The generated documentation will be in `docs/book/`.

## Development

To serve the documentation locally with live reload:

```bash
just docs-serve
```

The documentation will be available at http://localhost:3000 and will automatically reload when you edit source files.

## Structure

- `book.toml` - mdBook configuration
- `src/` - Documentation source files
  - `SUMMARY.md` - Table of contents
  - `overview.md` - System introduction
  - `architecture/` - Technical architecture docs
  - `api/` - API reference documentation
