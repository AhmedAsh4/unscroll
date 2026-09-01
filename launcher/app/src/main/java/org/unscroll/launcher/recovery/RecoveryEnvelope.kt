package org.unscroll.launcher.recovery

import java.security.MessageDigest
import java.util.SortedMap
import java.util.TreeMap

enum class ValidationError { SYNTAX, SCHEMA, FIELD, OPERATION, CHECKSUM, HISTORY, BASELINE, DEVICE_BINDING }
class RecoveryValidationException(val classification: ValidationError) : IllegalArgumentException(classification.name)

private const val MAX_INPUT = 65_536
private const val MAX_ITEMS = 512
private const val MAX_TEXT = 512

private sealed interface Json {
    data object Null : Json
    data class Bool(val value: Boolean) : Json
    data class Number(val value: Long) : Json
    data class Text(val value: String) : Json
    data class Array(val value: List<Json>) : Json
    data class Object(val value: SortedMap<String, Json>) : Json
}

class RecoveryEnvelopeV1 private constructor(private val value: Json) {
    val checksum: String get() = stringAt("checksum") ?: ""
    val activeAllowedPackages: Set<String>
        get() {
            validate()
            val policy = obj(part("active_policy") ?: fail(ValidationError.FIELD), ValidationError.FIELD)
            return arr(policy["allowed_packages"] ?: fail(ValidationError.FIELD), ValidationError.FIELD)
                .map { pkg -> text(pkg, ValidationError.FIELD) }
                .toSet()
        }
    val baselineLauncherPackage: String
        get() {
            validate()
            val baseline = obj(part("baseline") ?: fail(ValidationError.BASELINE), ValidationError.BASELINE)
            return text(baseline["baseline_launcher_package"] ?: fail(ValidationError.BASELINE), ValidationError.BASELINE)
        }
    val revision: Long get() { validate(); return numberAt("revision") ?: fail(ValidationError.FIELD) }

    fun canonicalJson(): String = canonical(value)
    fun computedChecksum(): String = sha256(canonicalWithout(value, "checksum"))

    fun requireStrictPrefixOf(newer: RecoveryEnvelopeV1) {
        validate(); newer.validate()
        if (part("baseline") != newer.part("baseline") || stringAt("baseline_id") != newer.stringAt("baseline_id")) fail(ValidationError.BASELINE)
        if (part("device_binding") != newer.part("device_binding")) fail(ValidationError.FIELD)
        val old = arrayAt("journal") ?: fail(ValidationError.SCHEMA)
        val next = newer.arrayAt("journal") ?: fail(ValidationError.SCHEMA)
        if ((newer.numberAt("revision") ?: fail(ValidationError.SCHEMA)) <= (numberAt("revision") ?: fail(ValidationError.SCHEMA)) || next.size < old.size || newer.stringAt("previous_revision_hash") != computedChecksum()) fail(ValidationError.HISTORY)
        var changed = next.size > old.size
        old.zip(next).forEach { (before, after) ->
            if (!sameEntryExceptState(before, after)) fail(ValidationError.HISTORY)
            if (entryState(before) != entryState(after)) {
                if (entryState(before) != "pending" || entryState(after) != "applied") fail(ValidationError.HISTORY)
                changed = true
            }
        }
        if (!changed) fail(ValidationError.HISTORY)
    }

    private fun validate() {
        val root = obj(value, ValidationError.SCHEMA)
        exact(root, setOf("active_policy", "baseline", "baseline_id", "checksum", "device_binding", "journal", "maintenance", "previous_revision_hash", "revision", "schema_version"), ValidationError.SCHEMA)
        if (stringAt("schema_version") != "recovery-v1") fail(ValidationError.SCHEMA)
        uuid(stringAt("baseline_id") ?: fail(ValidationError.FIELD))
        hash(stringAt("checksum") ?: fail(ValidationError.CHECKSUM))
        device(part("device_binding") ?: fail(ValidationError.SCHEMA))
        baseline(part("baseline") ?: fail(ValidationError.SCHEMA), stringAt("baseline_id")!!)
        policy(part("active_policy") ?: fail(ValidationError.SCHEMA), part("baseline")!!)
        maintenance(part("maintenance") ?: fail(ValidationError.SCHEMA))
        val revision = numberAt("revision") ?: fail(ValidationError.FIELD)
        if (revision == 0L) { if (part("previous_revision_hash") != Json.Null) fail(ValidationError.HISTORY) } else hash(text(part("previous_revision_hash") ?: fail(ValidationError.HISTORY), ValidationError.HISTORY))
        val journal = arrayAt("journal") ?: fail(ValidationError.SCHEMA)
        if (journal.size > MAX_ITEMS) fail(ValidationError.HISTORY)
        val ids = mutableSetOf<String>()
        var applied = 0L
        journal.forEach { entry ->
            val item = obj(entry, ValidationError.OPERATION)
            exact(item, setOf("id", "inverse", "operation", "state"), ValidationError.OPERATION)
            val id = text(item["id"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION); uuid(id)
            if (!ids.add(id)) fail(ValidationError.OPERATION)
            val state = text(item["state"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION)
            if (state !in setOf("pending", "applied")) fail(ValidationError.OPERATION)
            if (state == "applied") applied++
            val operation = item["operation"] ?: fail(ValidationError.OPERATION)
            val inverse = item["inverse"] ?: fail(ValidationError.OPERATION)
            val kind = operation(operation)
            if (operation(inverse) != kind || !isInverse(operation, inverse, kind)) fail(ValidationError.OPERATION)
        }
        if (revision != journal.size + applied) fail(ValidationError.HISTORY)
        if (checksum != computedChecksum()) fail(ValidationError.CHECKSUM)
    }

    private fun part(key: String): Json? = (value as? Json.Object)?.value?.get(key)
    private fun stringAt(key: String): String? = (part(key) as? Json.Text)?.value
    private fun numberAt(key: String): Long? = (part(key) as? Json.Number)?.value
    private fun arrayAt(key: String): List<Json>? = (part(key) as? Json.Array)?.value

    companion object {
        @JvmStatic fun parse(input: String): RecoveryEnvelopeV1 {
            if (input.length > MAX_INPUT) fail(ValidationError.FIELD)
            return RecoveryEnvelopeV1(Parser(input).parse()).also { it.validate() }
        }
        @JvmStatic fun parseUncheckedChecksum(input: String): RecoveryEnvelopeV1 {
            if (input.length > MAX_INPUT) fail(ValidationError.FIELD)
            return RecoveryEnvelopeV1(Parser(input).parse())
        }
        @JvmStatic fun parseForDevice(input: String, serial: String, fingerprint: String, userId: Long): RecoveryEnvelopeV1 {
            text(serial, ValidationError.DEVICE_BINDING); text(fingerprint, ValidationError.DEVICE_BINDING)
            if (userId !in 0..999_999) fail(ValidationError.DEVICE_BINDING)
            return parse(input).also { envelope ->
                val binding = obj(envelope.part("device_binding") ?: fail(ValidationError.SCHEMA), ValidationError.SCHEMA)
                if (binding["serial"] != Json.Text(serial) || binding["fingerprint"] != Json.Text(fingerprint) || binding["user_id"] != Json.Number(userId)) fail(ValidationError.DEVICE_BINDING)
            }
        }
    }
}

private fun entryState(entry: Json): String = text(obj(entry, ValidationError.HISTORY)["state"] ?: fail(ValidationError.HISTORY), ValidationError.HISTORY)
private fun sameEntryExceptState(left: Json, right: Json): Boolean = TreeMap(obj(left, ValidationError.HISTORY).filterKeys { it != "state" }) == TreeMap(obj(right, ValidationError.HISTORY).filterKeys { it != "state" })
private fun isInverse(operation: Json, inverse: Json, kind: String): Boolean {
    val forward = obj(operation, ValidationError.OPERATION)
    val backward = obj(inverse, ValidationError.OPERATION)
    fun equal(key: String): Boolean = forward[key] == backward[key]
    fun opposite(key: String): Boolean {
        val before = (forward[key] as? Json.Bool)?.value ?: return false
        val after = (backward[key] as? Json.Bool)?.value ?: return false
        return before != after
    }
    return when (kind) {
        "launcher_policy" -> !equal("allowed_packages")
        "package_suspension" -> equal("package_id") && equal("user_id") && opposite("suspended")
        "app_op" -> equal("package_id") && equal("user_id") && equal("op") && !equal("mode")
        "home" -> !equal("component")
        "maintenance" -> opposite("open")
        "cleanup" -> equal("target") && opposite("removed")
        else -> false
    }
}

private fun device(value: Json) { val o = obj(value, ValidationError.FIELD); exact(o, setOf("fingerprint", "serial", "user_id"), ValidationError.FIELD); text(text(o["serial"] ?: fail(ValidationError.FIELD), ValidationError.FIELD), ValidationError.FIELD); text(text(o["fingerprint"] ?: fail(ValidationError.FIELD), ValidationError.FIELD), ValidationError.FIELD); if (number(o["user_id"] ?: fail(ValidationError.FIELD)) > 999_999) fail(ValidationError.FIELD) }
private fun baseline(value: Json, id: String) { val o = obj(value, ValidationError.BASELINE); exact(o, setOf("baseline_id", "baseline_launcher_package", "initial_home_component", "initial_packages"), ValidationError.BASELINE); if (text(o["baseline_id"] ?: fail(ValidationError.BASELINE), ValidationError.BASELINE) != id) fail(ValidationError.BASELINE); pkg(text(o["baseline_launcher_package"] ?: fail(ValidationError.BASELINE), ValidationError.BASELINE)); component(text(o["initial_home_component"] ?: fail(ValidationError.BASELINE), ValidationError.BASELINE)); packageList(arr(o["initial_packages"] ?: fail(ValidationError.BASELINE), ValidationError.BASELINE), ValidationError.BASELINE, true) }
private fun policy(value: Json, baseline: Json) { val o = obj(value, ValidationError.FIELD); exact(o, setOf("allowed_packages", "baseline_launcher_package"), ValidationError.FIELD); if (o["baseline_launcher_package"] != obj(baseline, ValidationError.BASELINE)["baseline_launcher_package"]) fail(ValidationError.BASELINE); packageList(arr(o["allowed_packages"] ?: fail(ValidationError.FIELD), ValidationError.FIELD), ValidationError.FIELD, false) }
private fun maintenance(value: Json) { val o = obj(value, ValidationError.FIELD); exact(o, setOf("state"), ValidationError.FIELD); if (text(o["state"] ?: fail(ValidationError.FIELD), ValidationError.FIELD) !in setOf("closed", "open")) fail(ValidationError.FIELD) }
private fun packageList(values: List<Json>, error: ValidationError, records: Boolean) { if (values.size > MAX_ITEMS) fail(error); var previous = ""; values.forEach { value -> val name = if (records) { val o = obj(value, error); exact(o, setOf("package_id", "suspended", "user_id"), error); if (o["suspended"] !is Json.Bool || number(o["user_id"] ?: fail(error)) > 999_999) fail(error); text(o["package_id"] ?: fail(error), error) } else text(value, error); pkg(name); if (previous.isNotEmpty() && previous >= name) fail(error); previous = name } }
private fun operation(value: Json): String { val o = obj(value, ValidationError.OPERATION); val kind = text(o["kind"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION); when (kind) {
    "launcher_policy" -> { exact(o, setOf("allowed_packages", "kind"), ValidationError.OPERATION); packageList(arr(o["allowed_packages"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION), ValidationError.OPERATION, false) }
    "package_suspension" -> { exact(o, setOf("kind", "package_id", "suspended", "user_id"), ValidationError.OPERATION); pkg(text(o["package_id"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION)); if (o["suspended"] !is Json.Bool || number(o["user_id"] ?: fail(ValidationError.OPERATION)) > 999_999) fail(ValidationError.OPERATION) }
    "app_op" -> { exact(o, setOf("kind", "mode", "op", "package_id", "user_id"), ValidationError.OPERATION); pkg(text(o["package_id"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION)); text(text(o["op"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION), ValidationError.OPERATION); text(text(o["mode"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION), ValidationError.OPERATION); if (number(o["user_id"] ?: fail(ValidationError.OPERATION)) > 999_999) fail(ValidationError.OPERATION) }
    "home" -> { exact(o, setOf("component", "kind"), ValidationError.OPERATION); component(text(o["component"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION)) }
    "maintenance" -> { exact(o, setOf("kind", "open"), ValidationError.OPERATION); if (o["open"] !is Json.Bool) fail(ValidationError.OPERATION) }
    "cleanup" -> { exact(o, setOf("kind", "removed", "target"), ValidationError.OPERATION); if (o["removed"] !is Json.Bool || text(o["target"] ?: fail(ValidationError.OPERATION), ValidationError.OPERATION) !in setOf("private_envelope", "shared_envelope")) fail(ValidationError.OPERATION) }
    else -> fail(ValidationError.OPERATION)
}; return kind }

private fun obj(value: Json, error: ValidationError): SortedMap<String, Json> = (value as? Json.Object)?.value ?: fail(error)
private fun arr(value: Json, error: ValidationError): List<Json> = (value as? Json.Array)?.value ?: fail(error)
private fun text(value: Json, error: ValidationError): String = (value as? Json.Text)?.value ?: fail(error)
private fun number(value: Json): Long = (value as? Json.Number)?.value ?: fail(ValidationError.FIELD)
private fun exact(value: SortedMap<String, Json>, keys: Set<String>, error: ValidationError) { if (value.keys != keys) fail(error) }
private fun text(value: String, error: ValidationError) { if (value.isEmpty() || value.length > MAX_TEXT || value.any { it.code !in 0x20..0x7e }) fail(error) }
private fun pkg(value: String) { text(value, ValidationError.FIELD); val pieces = value.split('.'); if (value.length > 255 || pieces.size < 2 || pieces.any { part -> part.isEmpty() || part.withIndex().any { (index, char) -> char != '_' && char != '$' && (!char.isLetterOrDigit() || (index == 0 && !char.isLetter() && char != '_')) } }) fail(ValidationError.FIELD) }
private fun component(value: String) { text(value, ValidationError.FIELD); val pieces = value.split('/', limit = 2); if (pieces.size != 2 || pieces[1].isEmpty()) fail(ValidationError.FIELD); pkg(pieces[0]); if (pieces[1].any { !it.isLetterOrDigit() && it !in "._$" }) fail(ValidationError.FIELD) }
private fun uuid(value: String) { if (value.length != 36 || value.withIndex().any { (index, char) -> if (index in setOf(8,13,18,23)) char != '-' else !char.isDigit() && char !in 'a'..'f' }) fail(ValidationError.FIELD) }
private fun hash(value: String) { if (value.length != 64 || value.any { !it.isDigit() && it !in 'a'..'f' }) fail(ValidationError.CHECKSUM) }
private fun fail(error: ValidationError): Nothing = throw RecoveryValidationException(error)

private fun canonicalWithout(value: Json, key: String): String = when (value) { is Json.Object -> canonical(Json.Object(TreeMap(value.value.filterKeys { it != key }))); else -> canonical(value) }
private fun canonical(value: Json): String = when (value) { Json.Null -> "null"; is Json.Bool -> value.value.toString(); is Json.Number -> value.value.toString(); is Json.Text -> "\"" + value.value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t") + "\""; is Json.Array -> value.value.joinToString(separator = ",", prefix = "[", postfix = "]") { canonical(it) }; is Json.Object -> value.value.entries.joinToString(separator = ",", prefix = "{", postfix = "}") { "\"${it.key}\":${canonical(it.value)}" } }
private fun sha256(value: String): String = MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }

private class Parser(private val input: String) {
    private var at = 0
    fun parse(): Json { val result = value(0); ws(); if (at != input.length) fail(ValidationError.SYNTAX); return result }
    private fun value(depth: Int): Json { if (depth > 32) fail(ValidationError.SYNTAX); ws(); return when (peek()) { '{' -> obj(depth + 1); '[' -> array(depth + 1); '"' -> Json.Text(string()); 't' -> { word("true"); Json.Bool(true) }; 'f' -> { word("false"); Json.Bool(false) }; 'n' -> { word("null"); Json.Null }; in '0'..'9' -> Json.Number(number()); else -> fail(ValidationError.SYNTAX) } }
    private fun obj(depth: Int): Json { take('{'); ws(); val out = TreeMap<String, Json>(); if (peek() == '}') { at++; return Json.Object(out) }; while (true) { ws(); val key = string(); ws(); take(':'); val item = value(depth); if (out.put(key, item) != null) fail(ValidationError.SYNTAX); ws(); when (next()) { ',' -> Unit; '}' -> return Json.Object(out); else -> fail(ValidationError.SYNTAX) } } }
    private fun array(depth: Int): Json { take('['); ws(); val out = mutableListOf<Json>(); if (peek() == ']') { at++; return Json.Array(out) }; while (true) { if (out.size >= MAX_ITEMS) fail(ValidationError.FIELD); out += value(depth); ws(); when (next()) { ',' -> Unit; ']' -> return Json.Array(out); else -> fail(ValidationError.SYNTAX) } } }
    private fun string(): String { take('"'); val out = StringBuilder(); while (true) { when (val char = next()) { '"' -> return out.toString(); '\\' -> when (val escape = next()) { '"', '\\', '/' -> out.append(escape); 'b' -> out.append('\b'); 'f' -> out.append('\u000c'); 'n' -> out.append('\n'); 'r' -> out.append('\r'); 't' -> out.append('\t'); 'u' -> out.append(next().digitToInt(16).shl(12) + next().digitToInt(16).shl(8) + next().digitToInt(16).shl(4) + next().digitToInt(16)); else -> fail(ValidationError.SYNTAX) }; in '\u0000'..'\u001f' -> fail(ValidationError.SYNTAX); else -> out.append(char) }; if (out.length > MAX_TEXT) fail(ValidationError.FIELD) } }
    private fun number(): Long { val start = at; while (peek().isDigit()) at++; if (at - start > 1 && input[start] == '0') fail(ValidationError.SYNTAX); return input.substring(start, at).toLongOrNull() ?: fail(ValidationError.SYNTAX) }
    private fun word(word: String) { if (!input.startsWith(word, at)) fail(ValidationError.SYNTAX); at += word.length }
    private fun ws() { while (peek() in setOf(' ', '\n', '\r', '\t')) at++ }
    private fun take(char: Char) { if (next() != char) fail(ValidationError.SYNTAX) }
    private fun peek(): Char = input.getOrNull(at) ?: '\u0000'
    private fun next(): Char = input.getOrNull(at++) ?: fail(ValidationError.SYNTAX)
}
