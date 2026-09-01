# Unscroll bridge V1 wire contract

The `ContentProvider.call()` method name is `bridge-v1`. The provider requires
`android.permission.DUMP`; ordinary applications receive `forbidden`. Every
call is a Bundle containing exactly one UTF-8 JSON string named `request` and
returns exactly one UTF-8 JSON string named `response`. Missing, duplicate,
non-string, or over-64 KiB request values receive `invalid_request`.

## Request and response framing

Every request is an object with exactly `protocol_version`, `operation`, and
`arguments`; the version is the string `bridge-v1`. Unknown versions and
operations receive `unsupported`; unknown, missing, or wrong-type fields receive
`invalid_request`.

Every success response is an object with exactly `protocol_version`, `ok`, and
`result`; `protocol_version` is `bridge-v1`, `ok` is `true`, and `result` is
the exact success-result shape listed for that request's `operation` below. A
success response never includes `operation`, `arguments`, `error`, or extra
keys. Every failure response is an object with exactly `protocol_version`,
`ok`, and `error`; `protocol_version` is `bridge-v1`, `ok` is `false`, and
`error` is an object with exactly `code` and `message`. `code` is one of the
closed codes below, and `message` is printable ASCII, 1–512 bytes, intended
only for display. A failure response never includes `result` or extra keys.

The closed error codes are `forbidden`, `unsupported`, `invalid_request`,
`device_mismatch`, `not_found`, `conflict`, `corrupt_envelope`, and `internal`.
Messages are diagnostic-only; callers branch only on `code`.

## Shared value shapes

All JSON strings are printable ASCII and at most 512 bytes unless a tighter
bound appears below. A package ID matches
`^[A-Za-z_][A-Za-z0-9_$]*(\.[A-Za-z_][A-Za-z0-9_$]*)+$`; a binding is exactly
`{"serial":"string","fingerprint":"string","user_id":0}` with
`user_id` in 0–999999. A capabilities object has exactly the boolean members
`app_ops`, `home_selection`, `package_suspension`, and `recovery_storage`.

A catalog entry has exactly `package_id`, `activity_name`, `label`, `user_id`,
`launchable`, `enabled`, `suspended`, `activity_icon_available`,
`application_icon_available`, `icon_available`, and `protected_reason`;
`activity_name` is the fully-qualified launchable activity class used for the
icon request, and the state/icon fields are boolean.
`protected_reason` is null or 1–512 ASCII bytes. A cursor or stream ID is a
lowercase 32-character hexadecimal string; null is allowed only where stated.

## Operations

| operation | exact `arguments` object | exact success `result` object |
| --- | --- | --- |
| `health` | empty object | exactly `protocol_version`, `recovery_schema`, `launcher_package`; values are `bridge-v1`, `recovery-v1`, package ID |
| `device_facts` | exactly `serial`; the ADB shell-selected serial string | exactly `device_binding`, `capabilities` using the shared shapes |
| `catalog_page` | exactly `cursor`, `page_size`; cursor is null or cursor, page size is integer 1–100 | exactly `entries`, `next_cursor`; entries is 0–100 catalog entries, next cursor is null or cursor |
| `icon_stream` | exactly `package_id`, `activity_name`, `user_id` | exactly `byte_length`, `mime_type`, `sha256`, `stream_id`; length is 1–1048576, MIME is `image/png`, hash is 64 lowercase hex, stream ID follows shared shape |
| `read_envelope` | exactly `device_binding` | exactly `envelope`; envelope is canonical `recovery-v1` JSON string at most 65536 bytes |
| `write_envelope` | exactly `device_binding`, `envelope` | exactly `checksum`, `revision`; checksum is 64 lowercase hex and revision is integer 0–1024 |

For `icon_stream`, the consumer next calls `openFile` on exactly
`content://org.unscroll.launcher.bridge/bridge-v1/icon/<stream_id>` using read
mode, no query, fragment, or extra path segments. It receives exactly the PNG
bytes described by the response; EOF is required at `byte_length`, and the
SHA-256 must equal `sha256`. Any different path, mode, expired ID, hash/length
mismatch, or second read receives `not_found`. Catalog order is stable for one
cursor chain.

The ADB-shell caller supplies the selected serial only through `device_facts`
or the exact `device_binding` used by recovery operations. The provider derives
fingerprint and primary user locally; blank, generic, or mismatched bindings
return `device_mismatch`. `read_envelope` and `write_envelope` require equality
with that provider-derived binding. `write_envelope` validates
the complete V1 schema, canonical bytes, checksum, typed inverse semantics, and
binding before replacing private storage; corrupt bytes return
`corrupt_envelope`, and a non-extending revision returns `conflict`.

The bridge never accepts commands, executables, shell fragments, paths, SQL, or
caller-authored mutation operations. Recovery mutations are data in the closed
`recovery-v1` envelope only.
