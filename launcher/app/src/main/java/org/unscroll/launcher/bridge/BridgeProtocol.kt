package org.unscroll.launcher.bridge

enum class BridgeError(val code: String) { FORBIDDEN("forbidden"), UNSUPPORTED("unsupported"), INVALID_REQUEST("invalid_request"), DEVICE_MISMATCH("device_mismatch"), NOT_FOUND("not_found"), CONFLICT("conflict"), CORRUPT_ENVELOPE("corrupt_envelope"), INTERNAL("internal") }
class BridgeException(val error: BridgeError, message: String = error.code) : IllegalArgumentException(message)
data class BridgeRequest(val operation: String, val arguments: Map<String, Any?>)

object BridgeProtocol {
    const val VERSION = "bridge-v1"
    const val MAX_FRAME_BYTES = 262_144
    private const val MAX_ENVELOPE = 65_536
    private val operations = setOf("health", "device_facts", "catalog_page", "icon_stream", "read_envelope", "write_envelope")

    fun parseRequest(input: String): BridgeRequest {
        if (input.toByteArray(Charsets.UTF_8).size > MAX_FRAME_BYTES) throw BridgeException(BridgeError.INVALID_REQUEST)
        val root = Json.read(input) as? Map<*, *> ?: throw BridgeException(BridgeError.INVALID_REQUEST)
        val values = root.entries.associate { entry ->
            val key = entry.key as? String ?: throw BridgeException(BridgeError.INVALID_REQUEST)
            key to entry.value
        }
        if (values.keys != setOf("protocol_version", "operation", "arguments")) throw BridgeException(BridgeError.INVALID_REQUEST)
        val version = values["protocol_version"] as? String ?: throw BridgeException(BridgeError.INVALID_REQUEST)
        if (version != VERSION) throw BridgeException(BridgeError.UNSUPPORTED)
        val operation = values["operation"] as? String ?: throw BridgeException(BridgeError.INVALID_REQUEST)
        if (operation !in operations) throw BridgeException(BridgeError.UNSUPPORTED)
        val arguments = values["arguments"] as? Map<*, *> ?: throw BridgeException(BridgeError.INVALID_REQUEST)
        val parsed = arguments.entries.associate { entry ->
            val key = entry.key as? String ?: throw BridgeException(BridgeError.INVALID_REQUEST)
            key to entry.value
        }
        if (operation == "write_envelope" && ((parsed["envelope"] as? String)?.toByteArray(Charsets.UTF_8)?.size ?: 0) > MAX_ENVELOPE) throw BridgeException(BridgeError.INVALID_REQUEST)
        return BridgeRequest(operation, parsed)
    }

    fun parseError(input: String): BridgeError = try { parseRequest(input); BridgeError.INTERNAL } catch (error: BridgeException) { error.error }
    fun response(result: Map<String, Any?>): String = Json.write(mapOf("protocol_version" to VERSION, "ok" to true, "result" to result)).also { if (it.toByteArray(Charsets.UTF_8).size > MAX_FRAME_BYTES) throw BridgeException(BridgeError.INTERNAL) }
    fun failure(error: BridgeError, message: String = error.code): String = Json.write(mapOf("protocol_version" to VERSION, "ok" to false, "error" to mapOf("code" to error.code, "message" to message.take(512).filter { it.code in 0x20..0x7e }.ifBlank { error.code })))
    fun objectArgs(request: BridgeRequest, keys: Set<String>): Map<String, Any?> { if (request.arguments.keys != keys) throw BridgeException(BridgeError.INVALID_REQUEST); return request.arguments }
    fun string(value: Any?): String = value as? String ?: throw BridgeException(BridgeError.INVALID_REQUEST)
    fun number(value: Any?): Long = value as? Long ?: throw BridgeException(BridgeError.INVALID_REQUEST)

    private object Json {
        fun read(input: String): Any? = Parser(input).parse()
        fun write(value: Any?): String = when (value) {
            null -> "null"; is Boolean, is Long, is Int -> value.toString(); is String -> "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""
            is Map<*, *> -> value.entries.joinToString(",", "{", "}") { "${write(it.key as? String ?: error("key"))}:${write(it.value)}" }
            is Iterable<*> -> value.joinToString(",", "[", "]") { write(it) }
            else -> error("value")
        }
        private class Parser(private val input: String) {
            private var at = 0
            fun parse(): Any? { val value = value(0); ws(); if (at != input.length) bad(); return value }
            private fun value(depth: Int): Any? { if (depth > 32) bad(); ws(); return when (peek()) { '{' -> obj(depth + 1); '[' -> array(depth + 1); '"' -> string(); 't' -> word("true", true); 'f' -> word("false", false); 'n' -> word("null", null); in '0'..'9' -> number(); else -> bad() } }
            private fun obj(depth: Int): Map<String, Any?> { take('{'); ws(); val out = linkedMapOf<String, Any?>(); if (peek() == '}') { at++; return out }; while (true) { val key = string(); ws(); take(':'); if (out.containsKey(key)) bad(); out[key] = value(depth); ws(); when (next()) { ',' -> Unit; '}' -> return out; else -> bad() } } }
            private fun array(depth: Int): List<Any?> { take('['); ws(); val out = mutableListOf<Any?>(); if (peek() == ']') { at++; return out }; while (true) { if (out.size == 512) bad(); out += value(depth); ws(); when (next()) { ',' -> Unit; ']' -> return out; else -> bad() } } }
            private fun string(): String {
                take('"'); val out = StringBuilder()
                fun hex(): Int = next().digitToIntOrNull(16) ?: bad()
                fun unicode(): Int = hex().shl(12) + hex().shl(8) + hex().shl(4) + hex()
                while (true) when (val char = next()) {
                    '"' -> return out.toString()
                    '\\' -> when (next()) {
                        '"', '\\', '/' -> out.append(input[at - 1])
                        'b' -> out.append('\b'); 'f' -> out.append('\u000c'); 'n' -> out.append('\n'); 'r' -> out.append('\r'); 't' -> out.append('\t')
                        'u' -> { val code = unicode(); if (code in 0xd800..0xdbff) { if (next() != '\\' || next() != 'u') bad(); val low = unicode(); if (low !in 0xdc00..0xdfff) bad(); out.appendCodePoint(Character.toCodePoint(code.toChar(), low.toChar())) } else { if (code in 0xdc00..0xdfff) bad(); out.append(code.toChar()) } }
                        else -> bad()
                    }
                    in '\u0000'..'\u001f' -> bad()
                    else -> { if (char.code in 0xd800..0xdfff) bad(); out.append(char) }
                }
            }
            private fun number(): Long { val start = at; while (peek().isDigit()) at++; if (at - start > 1 && input[start] == '0') bad(); return input.substring(start, at).toLongOrNull() ?: bad() }
            private fun word(word: String, result: Any?): Any? { if (!input.startsWith(word, at)) bad(); at += word.length; return result }
            private fun ws() { while (peek() in " \n\r\t") at++ }; private fun take(char: Char) { if (next() != char) bad() }; private fun peek() = input.getOrNull(at) ?: '\u0000'; private fun next() = input.getOrNull(at++) ?: bad(); private fun bad(): Nothing = throw BridgeException(BridgeError.INVALID_REQUEST)
        }
    }
}
