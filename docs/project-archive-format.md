# Portable composition project archives (`.veproj`)

`.veproj` is the local, self-contained interchange format for a saved composition project. It contains the authoring document and the original bytes of every referenced source. It does not contain URLs, filesystem paths, database rows, credentials, render outputs, proxies, or remote-service identifiers.

The current archive format version is `1`. All integers are unsigned and big-endian.

## Binary layout

The fixed archive header is followed by one bounded canonical JSON manifest and then the declared media entries in manifest order.

| Field | Size | Version 1 value |
| --- | ---: | --- |
| Magic | 8 bytes | `VEPROJ\r\n` |
| Format version | `u16` | `1` |
| Flags | `u16` | `0` |
| Manifest length | `u32` | UTF-8 JSON byte length |
| Media entry count | `u32` | Must equal `manifest.sources.length` |
| Total media bytes | `u64` | Sum of all declared entry lengths |
| Manifest | variable | Canonical JSON described below |

Each media entry has this layout:

| Field | Size | Meaning |
| --- | ---: | --- |
| Entry marker | 4 bytes | `MED1` |
| Source ID length | `u16` | UTF-8 byte length |
| Filename length | `u16` | ASCII byte length |
| Media length | `u64` | Exact following media byte count |
| SHA-256 | 32 bytes | Raw digest of the media bytes |
| Source ID | variable | Must exactly match the manifest entry |
| Filename | variable | Safe single-component filename from the manifest |
| Media | variable | Original source bytes |

Trailing bytes are forbidden.

## Manifest

The manifest envelope is:

```json
{"format":"veproj","schemaVersion":1,"project":{"schemaVersion":2,"mode":"composition","name":"Example","document":{"schemaVersion":1,"sources":{},"tracks":[]}},"sources":[]}
```

Each source descriptor contains:

```json
{"sourceId":"source-a","filename":"source-a.wav","mediaType":"audio","title":"Dialogue","duration":1.25,"width":null,"height":null,"favorite":true,"tags":["voice"],"byteLength":12345,"sha256":"0123456789abcdef..."}
```

Canonical JSON rules for version 1:

- UTF-8 without a BOM and without insignificant whitespace;
- envelope fields use the order shown above;
- project fields are `schemaVersion`, `mode`, `name`, `document`;
- source fields use the order shown above;
- object keys inside the composition document are serialized lexicographically;
- source descriptors use the same order as the document's lexicographically serialized `sources` keys;
- SHA-256 is exactly 64 lowercase hexadecimal characters.

An importer reserializes the typed manifest and requires byte-for-byte equality, so non-canonical variants are rejected instead of being silently normalized.

## Limits and validation

- manifest: at most 3 MiB;
- composition document inside the manifest: at most 2 MiB;
- media entries: at most 32;
- one media entry: at most 2 GiB;
- total media bytes: at most 2 GiB;
- full archive: total media plus at most 4 MiB of framing and manifest data;
- project name: the same 256-byte bound as saved composition projects;
- library metadata: title up to 120 characters, up to 20 unique tags of 32 characters each;
- source IDs use the composition source-token grammar;
- filenames are ASCII single path components containing only letters, digits, `.`, `-`, and `_`.

The parser rejects unsupported versions/flags, malformed or non-canonical JSON, unsafe names, traversal, duplicate IDs or filenames, header/manifest discrepancies, truncated entries, checksum mismatches, and trailing bytes. Parsing streams media into private staging files and holds only the bounded manifest in memory.

## HTTP API

- `GET /api/composition-projects/:id/archive` returns `application/vnd.video-editor.project`, `Content-Disposition: attachment`, a content length, and `Cache-Control: no-store`.
- `POST /api/composition-projects/import` accepts exactly one multipart field named `file` and returns HTTP `201`:

```json
{"project":{"id":"...","schemaVersion":2,"mode":"composition","document":{},"sourceIds":[]},"sourceMapping":{"old-source-id":"new-source-id"}}
```

Export creates private hard-link snapshots before hashing and building the response, so deleting or renaming the original library entry cannot invalidate an in-flight response. Temporary files are removed when an error, timeout, cancellation, or dropped response body occurs.

Import fully parses, checksums, and locally probes every media entry with the existing restricted `ffprobe` path before publishing anything. It allocates collision-safe source IDs, rewrites document source keys, embedded source `id` values, and every `sourceId` reference, then publishes source files, library metadata, and the new project. Publication runs in an owned task so an HTTP disconnect cannot interrupt it midway; any pre-project failure triggers compensating source/library/metadata cleanup.
