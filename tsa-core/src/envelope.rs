//! JSON envelope 契约(SPEC §2.1(a))。
//!
//! 对照 Python 侧来源:
//! - `mcp/tools/tool_response.py`(`CANONICAL_VERDICTS`、`ENVELOPE_RULES`、`validate_tool_response`、`envelope_violations`)
//! - `mcp/tools/_response_builder.py`(工厂函数族、`InvalidVerdictError`)
//! - `mcp/server_utils/error_recovery.py`(错误信封规范化、K12 路径归一化、M14 language 回显)
//! - `tests/unit/cli/test_cli_envelope_contract.py` 的 `_CLI_ENVELOPE_KEYS`(CLI 4 键信封)
//!
//! 黄金对照物:`fixtures/envelope/*.json` 与 `fixtures/cli/cli_outputs.json`。
//! Stage 2-3:除 `cli_structure_json`(依赖 Unit 3 的 python_plugin AST 抽取)外全部实装;
//! 键序、错误消息、类型名称与 reason 码闭集均与 Python v3.1.2 逐字节对齐。

#![allow(unused_variables, dead_code)]

use serde_json::{json, Map, Value};

// ---------- 常量表(与 Python v3.1.2 逐字一致) ----------

/// 8 个 canonical verdict,顺序即 Python 侧 `sorted(CANONICAL_VERDICTS)`。
const CANONICAL_VERDICTS_SORTED: [&str; 8] =
    ["CAUTION", "ERROR", "INFO", "NOT_FOUND", "REVIEW", "SAFE", "UNSAFE", "WARN"];

/// `success=False` 时允许的有效 verdict(`FAILURE_VERDICTS`,排序形态)。
const FAILURE_VERDICTS: [&str; 2] = ["ERROR", "NOT_FOUND"];

/// `completeness` 合法状态(`COMPLETENESS_STATUSES`,排序形态)。
const COMPLETENESS_STATUSES: [&str; 3] = ["complete", "incomplete", "unknown"];

/// truncation 计数对(`TRUNCATION_COUNT_PAIRS`,顺序即声明序)。
const TRUNCATION_COUNT_PAIRS: [(&str, &str); 3] = [
    ("total_count", "returned_count"),
    ("total_matches", "returned_matches"),
    ("caller_count", "callers_listed"),
];

/// 必须为 bool 的 truncation 类标志(`_TRUNCATION_FLAGS`)。
const TRUNCATION_FLAGS: [&str; 2] = ["truncated", "skipped_files_truncated"];

/// 错误身份字段优先序(`_IDENTIFIER_FIELDS`):file_path → symbol → query → roots[0] → mode。
const IDENTIFIER_FIELDS: [&str; 5] = ["file_path", "symbol", "query", "roots", "mode"];

/// K12 路径回显归一化的键(`_FILE_PATH_ECHO_KEYS`)。
const FILE_PATH_ECHO_KEYS: [&str; 2] = ["file_path", "source_file"];

/// summary_line 兜底后缀视为 success 类的 verdict(`_SUCCESS_LIKE_VERDICTS`)。
const SUCCESS_LIKE_VERDICTS: [&str; 4] = ["INFO", "SAFE", "OK", "SUCCESS"];

/// 错误恢复子串规则(15 条,`_ERROR_RECOVERY_HINTS`;按序首中即停,消息已 lower)。
const ERROR_RECOVERY_HINTS: &[(&str, &str, &str, &str)] = &[
    (
        "not found",
        "file_not_found",
        "The file does not exist at the given path. Verify it with the host agent's live file-search capability. Rebuild the index before using structure action=sitemap for indexed source structure.",
        "",
    ),
    (
        "no such file",
        "file_not_found",
        "The file does not exist at the given path. Verify it with the host agent's live file-search capability. Rebuild the index before using structure action=sitemap for indexed source structure.",
        "",
    ),
    (
        "unsupported language",
        "language_unsupported",
        "The file extension is not recognized. Check supported languages with --show-supported-languages.",
        "",
    ),
    (
        "project root",
        "project_not_set",
        "Project root has not been set. Call set_project_path first.",
        "set_project_path",
    ),
    (
        "outside project boundary",
        "security_violation",
        "The file is outside the project boundary. Use a path relative to the project root.",
        "",
    ),
    (
        // #668:blast_radius 的 "function_names is required ..." 消息含 "required",
        //必须排在通用 required 规则前,否则丢失 file-level xref 指引。
        "blast_radius mode",
        "validation",
        "blast_radius needs function_names (a list of function names). For file-level dependents, use nav action=xref mode=file instead.",
        "nav action=xref mode=file",
    ),
    (
        "required",
        "validation",
        "A required parameter is missing. Check the tool schema and provide all required fields.",
        "",
    ),
    (
        "must be",
        "validation",
        "A parameter has an invalid value. Check the tool schema for valid options.",
        "",
    ),
    (
        // invalid_enum_error() 格式(#449):"Invalid <param>: '<got>'. Valid values: a, b, c"
        "valid values:",
        "validation",
        "A parameter has an invalid value. The error message lists the valid values — pick one and retry.",
        "",
    ),
    (
        "memory",
        "resource_exhausted",
        "The operation ran out of memory. Try analyzing a smaller scope or use suppress_output=true.",
        "",
    ),
    (
        "timed out",
        "timeout",
        "The operation timed out. Try reducing the scope (limit, max_count) or targeting a specific file.",
        "",
    ),
    (
        "regex parse error",
        "subprocess",
        "The regex pattern is invalid. Use a valid pattern or pass `glob=True` to match literal globs.",
        "",
    ),
    (
        // 需要 AST cache 的工具在建索引前报 "AST cache is empty";对策就是建索引。
        "cache is empty",
        "index_not_ready",
        "The index has not been built. Run index action=auto, then retry.",
        "index",
    ),
    (
        // 查询/选择器语言(hyphae、TQL、UQL)以 "... syntax error: ..." 报格式错;那是调用方的输入。
        "syntax error",
        "validation",
        "The query or selector could not be parsed. Fix its syntax and retry.",
        "",
    ),
    (
        "permission denied",
        "permission_denied",
        "Permission denied. Check file ownership and that the path is under project_root.",
        "",
    ),
];

/// 异常类名 → 机器可读 error_type 的回退表(`_EXC_CLASS_TO_TYPE`);未列出者兜底 `internal`。
const EXC_CLASS_TO_TYPE: &[(&str, &str)] = &[
    ("FileNotFoundError", "file_not_found"),
    ("NotADirectoryError", "file_not_found"),
    ("IsADirectoryError", "validation"),
    ("PermissionError", "permission_denied"),
    ("ValueError", "validation"),
    ("TypeError", "validation"),
    ("KeyError", "validation"),
    ("IndexError", "validation"),
    ("TimeoutError", "timeout"),
    ("AnalysisError", "internal"),
    ("MCPError", "internal"),
    ("RuntimeError", "internal"),
    ("OSError", "internal"),
    ("AttributeError", "internal"),
];

/// known completeness reason 闭集(42 个;Python 侧 `COMPLETENESS_REASONS | UQL_REASONS |
/// INDEX_COMPLETENESS_REASONS` 的排序形态)。排序以启用二分查找。
const KNOWN_COMPLETENESS_REASONS: &[&str] = &[
    "ACCESSOR_NOT_CALL_EDGES",
    "ACCESSOR_UNCHECKED",
    "CALLEES_COVERAGE_UNREPORTED",
    "CALLEE_UNRESOLVED",
    "CURSOR_GENERATION_UNVERIFIED",
    "DECORATOR_MAY_REGISTER",
    "EMPTY_EVIDENCE_BASE",
    "INDEX_BUILD_IN_PROGRESS",
    "INDEX_NOT_CERTIFIED",
    "INDEX_SCOPE_INCOMPLETE",
    "INDEX_SNAPSHOT_UNAVAILABLE",
    "MEMBERSHIP_UNKNOWN",
    "MISSING_INDEX",
    "NAME_ONLY_INBOUND",
    "NAME_ONLY_INBOUND_UNCHECKED",
    "NON_CALL_REFERENCES",
    "NON_CALL_REFERENCES_UNCHECKED",
    "PRODUCER_COVERAGE_MISSING",
    "PROJECT_ROOT_MISSING",
    "QUERY_NOT_EXECUTED",
    "RELATION_DEPTH_LIMIT",
    "RELATION_ERROR",
    "RELATION_LIMIT",
    "RELATION_NODE_BUDGET",
    "SELECTOR_PREDICATE_UNPROVEN",
    "SELECT_CAP_REACHED",
    "SOURCE_INDEX_MISMATCH",
    "SOURCE_RECHECK_DEADLINE",
    "SOURCE_SCOPE_UNKNOWN",
    "SYMBOL_DEFINITION_UNCHECKED",
    "SYMBOL_IN_EXCLUDED_FILE",
    "SYMBOL_NOT_DEFINED",
    "TEST_HELPER_DEPTH",
    "UNATTRIBUTED_CALL_SITES",
    "UNATTRIBUTED_CALL_SITES_UNCHECKED",
    "UNINDEXED_SOURCE_FILES",
    "UNINDEXED_SOURCE_UNCHECKED",
    "UNRESOLVED_INBOUND",
    "UNRESOLVED_INBOUND_UNCHECKED",
    "UNRESOLVED_IN_DECLARING_FILE",
    "UPSTREAM_INCOMPLETE",
    "UPSTREAM_UNKNOWN",
];

// ---------- 类型定义 ----------

/// canonical verdict 枚举(8 个,SPEC §2.1;与 Python 侧两处 `CANONICAL_VERDICTS` frozenset 一致)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalVerdict {
    /// SAFE
    Safe,
    /// REVIEW
    Review,
    /// CAUTION
    Caution,
    /// UNSAFE
    Unsafe,
    /// INFO
    Info,
    /// WARN
    Warn,
    /// ERROR
    Error,
    /// NOT_FOUND
    NotFound,
}

impl CanonicalVerdict {
    /// 序列化用的规范字符串(与 fixture `canonical_verdicts` 逐字一致)。
    pub fn as_str(&self) -> &'static str {
        match self {
            CanonicalVerdict::Safe => "SAFE",
            CanonicalVerdict::Review => "REVIEW",
            CanonicalVerdict::Caution => "CAUTION",
            CanonicalVerdict::Unsafe => "UNSAFE",
            CanonicalVerdict::Info => "INFO",
            CanonicalVerdict::Warn => "WARN",
            CanonicalVerdict::Error => "ERROR",
            CanonicalVerdict::NotFound => "NOT_FOUND",
        }
    }
}

/// `build_response` 收到非 canonical verdict 时返回的错误。
///
/// 对应 Python 侧 `InvalidVerdictError(ValueError)`:`message` 必须与 fixture
/// `response_builder_invalid.json` 逐字一致(含非法值与 `sorted(CANONICAL_VERDICTS)`)。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct InvalidVerdictError {
    /// 非法的 verdict 原值
    pub verdict: String,
    /// 完整错误消息(格式见 fixture,例:`verdict='OK' not in canonical set [...]. Use build_error() ...`)
    pub message: String,
}

/// 一条 envelope 违规(与 fixture 中 `[rule, message]` 二元组对应)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeViolation {
    /// 违规规则名(必须来自 `ENVELOPE_RULES` 闭集,如 `failure-verdict`)
    pub rule: String,
    /// 人类可读的违规说明
    pub message: String,
}

impl EnvelopeViolation {
    /// 转为与 fixture 元组形态一致的 `(rule, message)`。
    pub fn as_tuple(&self) -> (String, String) {
        (self.rule.clone(), self.message.clone())
    }
}

/// `error_recovery._classify` 的分类结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorClassification {
    /// 机器可读错误类型(如 `file_not_found`、`validation`、`internal`)
    pub error_type: String,
    /// 人类可读恢复提示(agent_summary.next_step 同源)
    pub recovery_hint: String,
    /// 建议的后续工具(空串 = 无建议;如 `set_project_path`、`index`)
    pub suggested_tool: String,
}

// ---------- Python 值形态助手(消息逐字节对齐的关键) ----------

/// CPython `str` 的 `!r` 形态:默认单引号;含单引号且不含双引号时改用双引号。
fn py_repr_str(s: &str) -> String {
    if s.contains('\'') {
        if !s.contains('"') {
            return format!("\"{s}\"");
        }
        return format!("'{}'", s.replace('\'', "\\'"));
    }
    format!("'{s}'")
}

/// CPython `repr(value)` 近似:str/bool/None/int/float/list 与 Python 逐字一致;
/// dict 形态契约测试未覆盖,退化为近似输出。
fn py_repr(value: &Value) -> String {
    match value {
        Value::String(s) => py_repr_str(s),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Null => "None".to_string(),
        Value::Number(n) => serde_json::to_string(n).unwrap_or_else(|_| "0".to_string()),
        Value::Array(items) => {
            format!("[{}]", items.iter().map(py_repr).collect::<Vec<_>>().join(", "))
        }
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}: {}", py_repr_str(k), py_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// CPython `str(value)`:仅 str 与 repr 不同(无引号),其余与 repr 一致。
fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => py_repr(other),
    }
}

/// Python 侧 `type(value).__name__`(断言消息中的类型名)。
fn py_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        // Python 区分 int / float;serde_json 的整数存储即 int 形态
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int"
            } else {
                "float"
            }
        }
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Python 真值判定(空串/0/false/None/空容器为假)。
fn py_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python `isinstance(value, int) and not isinstance(value, bool)` 的整数值。
fn py_int(value: Option<&Value>) -> Option<i64> {
    let number = value?.as_number()?;
    if let Some(i) = number.as_i64() {
        return Some(i);
    }
    // Python int 任意精度;u64 超出 i64 时饱和放弃(契约测试未覆盖)
    number.as_u64().and_then(|u| i64::try_from(u).ok())
}

/// 字符串列表的 Python repr 形态(如 `['ERROR', 'NOT_FOUND']`)。
fn str_list_repr(items: &[&str]) -> String {
    format!("[{}]", items.iter().map(|s| py_repr_str(s)).collect::<Vec<_>>().join(", "))
}

/// verdict 是否为「真值」:`n/a`/空串/非 str 均视为未设置(`_real`)。
fn real_verdict(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(s)) if !s.is_empty() && s != "n/a" => Some(s.as_str()),
        _ => None,
    }
}

/// 消息前导 UPPER_SNAKE 错误码(`_LEADING_CODE`:`^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(?::|$)`)。
///
/// 手写等价匹配:首段 `[A-Z][A-Z0-9]*` 后必须跟至少一个非空 `_段`;捕获组之后
/// 必须是 `:` 或串尾。贪心消费后若终止符不匹配则整体不匹配(与回溯语义一致:
/// 组内任何更短的截断点后继字符均不属于 `:`/串尾)。
fn leading_error_code(message: &str) -> Option<&str> {
    let bytes = message.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_uppercase() {
        return None;
    }
    // 首段:首字符大写,其后贪心消费大写字母与数字
    let mut i = 1;
    while i < bytes.len() && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit()) {
        i += 1;
    }
    // 至少一个 `_[A-Z0-9]+` 段(段体非空)
    let mut end = i;
    let mut matched = false;
    while i < bytes.len() && bytes[i] == b'_' {
        i += 1;
        let seg_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit()) {
            i += 1;
        }
        if i == seg_start {
            break; // 空段:回退到最后一个完整段
        }
        end = i;
        matched = true;
    }
    if !matched {
        return None;
    }
    // 捕获组之后必须是 ':' 或串尾
    if end == bytes.len() || bytes[end] == b':' {
        Some(&message[..end])
    } else {
        None
    }
}

/// reason 码取最后一个 `:` 之后段(UQL 的 `<stage>:<CODE>` 前缀剥离;无冒号则原样)。
fn reason_code_segment(reason: &str) -> &str {
    match reason.rfind(':') {
        Some(pos) => &reason[pos + 1..],
        None => reason,
    }
}

// ---------- 工厂函数族(_response_builder.py) ----------

/// 8 个 canonical verdict 字符串(SPEC §2.1;顺序即 Python 侧 `sorted(CANONICAL_VERDICTS)`):
/// `["CAUTION", "ERROR", "INFO", "NOT_FOUND", "REVIEW", "SAFE", "UNSAFE", "WARN"]`。
pub fn canonical_verdicts() -> &'static [&'static str] {
    &CANONICAL_VERDICTS_SORTED
}

/// 构造与 Python 逐字一致的 `InvalidVerdictError`。
fn invalid_verdict_error(verdict: &str) -> InvalidVerdictError {
    InvalidVerdictError {
        verdict: verdict.to_string(),
        message: format!(
            "verdict={} not in canonical set {}. Use build_error() for error responses.",
            py_repr_str(verdict),
            str_list_repr(&CANONICAL_VERDICTS_SORTED)
        ),
    }
}

/// `build_response(*, verdict, success=True, warnings=None, **fields)`。
///
/// 输出 `{"success", "verdict", **fields}`,warnings 非空时追加 `"warnings"`(拷贝为 list)。
/// verdict 不在 canonical 集合时立即返回 `Err(InvalidVerdictError)`。
pub fn build_response(
    verdict: &str,
    success: bool,
    warnings: Option<&[&str]>,
    fields: Map<String, Value>,
) -> Result<Value, InvalidVerdictError> {
    if !CANONICAL_VERDICTS_SORTED.contains(&verdict) {
        return Err(invalid_verdict_error(verdict));
    }
    // Python: {"success": ..., "verdict": ..., **fields};warnings 非空时追加
    let mut envelope = Map::new();
    envelope.insert("success".to_string(), json!(success));
    envelope.insert("verdict".to_string(), json!(verdict));
    for (key, value) in fields {
        envelope.insert(key, value);
    }
    if let Some(warnings) = warnings {
        if !warnings.is_empty() {
            envelope.insert(
                "warnings".to_string(),
                Value::Array(warnings.iter().map(|w| json!(w)).collect()),
            );
        }
    }
    Ok(Value::Object(envelope))
}

/// `build_error(*, error, verdict="ERROR", **fields)`。
///
/// 输出 `{"success": False, "verdict", "error", **fields}`;verdict 同样过 canonical 校验。
pub fn build_error(
    error: &str,
    verdict: &str,
    fields: Map<String, Value>,
) -> Result<Value, InvalidVerdictError> {
    // Python: build_response(verdict=verdict, success=False, error=error, **fields)
    // error 关键字先于 **fields,故 error 键排在 fields 之前
    let mut merged = Map::new();
    merged.insert("error".to_string(), json!(error));
    for (key, value) in fields {
        merged.insert(key, value);
    }
    build_response(verdict, false, None, merged)
}

/// `build_error_response(tool_name, error, verdict="ERROR", **fields)`。
///
/// 同 `build_error` 并追加 `"tool": tool_name`。
pub fn build_error_response(
    tool_name: &str,
    error: &str,
    verdict: &str,
    fields: Map<String, Value>,
) -> Result<Value, InvalidVerdictError> {
    // Python: build_error(error=error, verdict=verdict, tool=tool_name, **fields)
    let mut merged = Map::new();
    merged.insert("tool".to_string(), json!(tool_name));
    for (key, value) in fields {
        merged.insert(key, value);
    }
    build_error(error, verdict, merged)
}

/// `build_success_response(tool_name, data, verdict="INFO", duration_ms=None, **fields)`。
///
/// 输出 `{"success": True, "verdict", "tool", **data, **fields}`,`duration_ms` 非 None 时追加。
pub fn build_success_response(
    tool_name: &str,
    data: Map<String, Value>,
    verdict: &str,
    duration_ms: Option<f64>,
) -> Result<Value, InvalidVerdictError> {
    // Python: extra = {"tool": tool_name, **data, **fields};duration_ms 非 None 追加
    let mut extra = Map::new();
    extra.insert("tool".to_string(), json!(tool_name));
    for (key, value) in data {
        extra.insert(key, value);
    }
    if let Some(ms) = duration_ms {
        extra.insert("duration_ms".to_string(), json!(ms));
    }
    build_response(verdict, true, None, extra)
}

/// `build_agent_summary(verdict, message, recommendations=None, **fields)`。
///
/// 输出 `{"success": True, "verdict", "message", **fields}`,recommendations 非空时追加。
pub fn build_agent_summary(
    verdict: &str,
    message: &str,
    recommendations: Option<&[&str]>,
) -> Result<Value, InvalidVerdictError> {
    // Python: extra = {"message": message, **fields};recommendations 非空追加
    let mut extra = Map::new();
    extra.insert("message".to_string(), json!(message));
    if let Some(recommendations) = recommendations {
        if !recommendations.is_empty() {
            extra.insert(
                "recommendations".to_string(),
                Value::Array(recommendations.iter().map(|r| json!(r)).collect()),
            );
        }
    }
    build_response(verdict, true, None, extra)
}

// ---------- validate_tool_response / envelope_violations(tool_response.py) ----------

/// `validate_tool_response(payload, tool_name)` 的 Rust 形态。
///
/// Python 侧违规 raise `AssertionError`;此处返回 `Err(断言消息)`。
/// 判据(payload 必须 dict;`success` 必存在且为 bool;失败时必须有 `error: str`;
/// `verdict` 存在时必须 canonical)对应规则 1 `base`。
/// 消息格式见 fixture `envelope_validation.json` 的 `assertion_error` 字段。
pub fn validate_tool_response(payload: &Value, tool_name: &str) -> Result<(), String> {
    let Some(map) = payload.as_object() else {
        return Err(format!(
            "{tool_name}: ToolResponse must be a dict, got {}",
            py_type_name(payload)
        ));
    };
    let Some(success) = map.get("success") else {
        return Err(format!("{tool_name}: ToolResponse must include a 'success' key"));
    };
    if !success.is_boolean() {
        return Err(format!(
            "{tool_name}: ToolResponse['success'] must be bool, got {}",
            py_type_name(success)
        ));
    }
    if success.as_bool() == Some(false) {
        // 失败必须给出 error;成功允许完全没有
        let Some(error) = map.get("error") else {
            return Err(format!(
                "{tool_name}: ToolResponse with success=False must include 'error'"
            ));
        };
        if !error.is_string() {
            return Err(format!(
                "{tool_name}: ToolResponse['error'] must be str, got {}",
                py_type_name(error)
            ));
        }
    }
    // verdict 在场时必须 canonical(CLEAN/NEEDS_REVIEW 等会造成 agent 静默失调)
    if let Some(verdict) = map.get("verdict") {
        if !verdict.is_string() {
            return Err(format!(
                "{tool_name}: ToolResponse['verdict'] must be str, got {}",
                py_type_name(verdict)
            ));
        }
        let verdict = verdict.as_str().expect("上文已确认为 str");
        if !CANONICAL_VERDICTS_SORTED.contains(&verdict) {
            return Err(format!(
                "{tool_name}: ToolResponse['verdict']={} is not in the canonical set {}",
                py_repr_str(verdict),
                str_list_repr(&CANONICAL_VERDICTS_SORTED)
            ));
        }
    }
    Ok(())
}

/// verdict / agent_summary 一致性规则(`_verdict_violations`)。
fn verdict_violations(payload: &Map<String, Value>) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let top = real_verdict(payload.get("verdict"));
    let mut summary_map: Option<&Map<String, Value>> = None;
    if let Some(summary) = payload.get("agent_summary") {
        if !summary.is_object() {
            found.push((
                "agent-summary-shape".to_string(),
                format!("agent_summary must be a dict, got {}", py_type_name(summary)),
            ));
        } else {
            summary_map = summary.as_object();
        }
    }
    let inner = summary_map.and_then(|m| real_verdict(m.get("verdict")));
    if let Some(inner) = inner {
        if !CANONICAL_VERDICTS_SORTED.contains(&inner) {
            found.push((
                "verdict-canonical".to_string(),
                format!("agent_summary.verdict={} is not canonical", py_repr_str(inner)),
            ));
        }
    }
    if let (Some(top), Some(inner)) = (top, inner) {
        if top != inner {
            found.push((
                "verdict-mirror".to_string(),
                format!(
                    "verdict={} but agent_summary.verdict={}",
                    py_repr_str(top),
                    py_repr_str(inner)
                ),
            ));
        }
    }
    let effective = top.or(inner);
    let success = payload.get("success").and_then(Value::as_bool);
    if success == Some(false) {
        if let Some(effective) = effective {
            if !FAILURE_VERDICTS.contains(&effective) {
                found.push((
                    "failure-verdict".to_string(),
                    format!(
                        "success=False with verdict={}; expected one of {}",
                        py_repr_str(effective),
                        str_list_repr(&FAILURE_VERDICTS)
                    ),
                ));
            }
        }
    }
    if success == Some(true) && effective == Some("ERROR") {
        found.push((
            "success-verdict".to_string(),
            "success=True with verdict='ERROR'".to_string(),
        ));
    }
    found
}

/// 失败响应机器码规则(`_error_code_violations`)。
fn error_code_violations(payload: &Map<String, Value>) -> Vec<(String, String)> {
    if payload.get("success").and_then(Value::as_bool) != Some(false) {
        return Vec::new();
    }
    for key in ["error_code", "error_type"] {
        if let Some(Value::String(s)) = payload.get(key) {
            if !s.is_empty() {
                return Vec::new();
            }
        }
    }
    vec![(
        "error-code".to_string(),
        "success=False without a machine-readable error_code/error_type".to_string(),
    )]
}

/// completeness 状态与原因码规则(`_completeness_violations`)。
fn completeness_violations(payload: &Map<String, Value>) -> Vec<(String, String)> {
    let Some(status_value) = payload.get("completeness") else {
        return Vec::new();
    };
    let status_known = status_value
        .as_str()
        .is_some_and(|s| COMPLETENESS_STATUSES.contains(&s));
    if !status_known {
        return vec![(
            "completeness-status".to_string(),
            format!(
                "completeness={} is not one of {}",
                py_repr(status_value),
                str_list_repr(&COMPLETENESS_STATUSES)
            ),
        )];
    }
    let status = status_value.as_str().expect("上文已确认为合法状态 str");
    let reasons = payload.get("completeness_reasons");
    let primary = payload.get("completeness_reason");
    if let Some(reasons) = reasons {
        if !reasons.is_array() {
            return vec![(
                "completeness-reasons".to_string(),
                "completeness_reasons must be a list".to_string(),
            )];
        }
    }
    let mut listed: Vec<&Value> = reasons
        .and_then(Value::as_array)
        .map(|list| list.iter().collect())
        .unwrap_or_default();
    // primary 在场而列表为空时,视作 [primary]
    if primary.is_some_and(|p| !p.is_null()) && listed.is_empty() {
        listed = vec![primary.expect("上文已确认在场")];
    }
    let mut found: Vec<(String, String)> = Vec::new();
    if status == "complete" && !listed.is_empty() {
        found.push((
            "completeness-reasons".to_string(),
            format!(
                "completeness='complete' carries reasons [{}]",
                listed.iter().map(|v| py_repr(v)).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    if status != "complete" && listed.is_empty() {
        found.push((
            "completeness-reasons".to_string(),
            format!("completeness={} names no reason code", py_repr_str(status)),
        ));
    }
    // primary 必须与 completeness_reasons[0] 一致(reasons 为非空 list 时才比较)
    if let Some(primary) = primary.filter(|p| !p.is_null()) {
        if let Some(Value::Array(reasons_list)) = reasons {
            if !reasons_list.is_empty() && *primary != reasons_list[0] {
                found.push((
                    "completeness-reasons".to_string(),
                    format!(
                        "completeness_reason={} is not completeness_reasons[0]",
                        py_repr(primary)
                    ),
                ));
            }
        }
    }
    // 原因码闭集校验(取最后一个 ':' 之后段;去重排序后报告)
    let mut unknown: Vec<String> = listed
        .iter()
        .map(|r| py_str(r))
        .filter(|r| {
            KNOWN_COMPLETENESS_REASONS
                .binary_search(&reason_code_segment(r))
                .is_err()
        })
        .collect();
    unknown.sort();
    unknown.dedup();
    if !unknown.is_empty() {
        let unknown_refs: Vec<&str> = unknown.iter().map(String::as_str).collect();
        found.push((
            "completeness-reason-code".to_string(),
            format!("unknown completeness reason codes {}", str_list_repr(&unknown_refs)),
        ));
    }
    found
}

/// truncation 规则(`_truncation_violations`)。
fn truncation_violations(payload: &Map<String, Value>) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for flag in TRUNCATION_FLAGS {
        if let Some(value) = payload.get(flag) {
            if !value.is_boolean() {
                found.push((
                    "truncation".to_string(),
                    format!("{flag} must be bool, got {}", py_type_name(value)),
                ));
            }
        }
    }
    let truncated = payload.get("truncated").and_then(Value::as_bool);
    for (total_key, listed_key) in TRUNCATION_COUNT_PAIRS {
        let (Some(total), Some(listed)) = (
            py_int(payload.get(total_key)),
            py_int(payload.get(listed_key)),
        ) else {
            continue;
        };
        if listed > total {
            found.push((
                "truncation".to_string(),
                format!("{listed_key}={listed} exceeds {total_key}={total}"),
            ));
        } else if listed < total && truncated == Some(false) {
            found.push((
                "truncation".to_string(),
                format!("truncated=False but {listed_key}={listed} < {total_key}={total}"),
            ));
        }
    }
    found
}

/// `envelope_violations(payload, tool_name)`:返回全部 `(rule, message)` 违规,不抛异常;空列表 = 合规。
///
/// 必须覆盖 11 条 envelope 规则(fixture `rules_actually_fired` 已逐一验证)。
pub fn envelope_violations(payload: &Value, tool_name: &str) -> Vec<(String, String)> {
    // 规则 1 base:校验失败即短路返回
    if let Err(message) = validate_tool_response(payload, tool_name) {
        return vec![("base".to_string(), message)];
    }
    let map = payload.as_object().expect("base 通过后必为对象");
    let mut found = verdict_violations(map);
    found.extend(error_code_violations(map));
    found.extend(completeness_violations(map));
    found.extend(truncation_violations(map));
    found
}

// ---------- 错误恢复(error_recovery.py) ----------

/// `_classify` 核心:15 条子串规则按序首中即停;未中时查异常类回退表;最终兜底 `internal`。
///
/// `exc_class` 为 `None` 或空串时对应 Python 侧不传 hint 的调用形态(直接兜底)。
fn classify_with_hint(error_msg: &str, exc_class: Option<&str>) -> ErrorClassification {
    let msg = error_msg.to_lowercase();
    for (pattern, error_type, recovery_hint, suggested_tool) in ERROR_RECOVERY_HINTS {
        if msg.contains(pattern) {
            return ErrorClassification {
                error_type: error_type.to_string(),
                recovery_hint: recovery_hint.to_string(),
                suggested_tool: suggested_tool.to_string(),
            };
        }
    }
    // 异常类回退表;未列出者兜底 internal
    let error_type = exc_class
        .filter(|class| !class.is_empty())
        .and_then(|class| EXC_CLASS_TO_TYPE.iter().find(|(name, _)| *name == class))
        .map_or_else(|| "internal".to_string(), |(_, t)| t.to_string());
    ErrorClassification {
        error_type,
        recovery_hint: "Review the error message and adjust your request.".to_string(),
        suggested_tool: String::new(),
    }
}

/// `error_recovery._classify(sanitized_message, exception_class)`。
///
/// 消息文本优先于异常类(15 条子串规则按序首中即停),异常类回退表次之,最终兜底 `internal`。
/// 对照 fixture `error_recovery.json` 的 `error_cases` 字段(12 个用例)。
pub fn classify_error(error_message: &str, exc_class: &str) -> ErrorClassification {
    classify_with_hint(error_message, Some(exc_class))
}

/// `_summary_line`:identifier 在场时 `<tool>: <type> — <identifier>`(em dash),否则 `<tool>: <type>`。
fn summary_line_for(tool_name: &str, error_type: &str, identifier: Option<&str>) -> String {
    match identifier {
        Some(identifier) => format!("{tool_name}: {error_type} — {identifier}"),
        None => format!("{tool_name}: {error_type}"),
    }
}

/// `_pick_identifier`:按 `file_path → symbol → query → roots[0] → mode` 取最有信息量的标识。
fn pick_identifier(arguments: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(map) = arguments.and_then(Value::as_object) else {
        return (None, None);
    };
    for key in IDENTIFIER_FIELDS {
        if let Some(Value::String(s)) = map.get(key) {
            if !s.is_empty() {
                return (Some(key.to_string()), Some(s.clone()));
            }
        }
        if key == "roots" {
            if let Some(Value::Array(list)) = map.get(key) {
                if let Some(first) = list.first() {
                    if let Value::String(s) = first {
                        if !s.is_empty() {
                            return (Some("roots".to_string()), Some(s.clone()));
                        }
                    }
                }
            }
        }
    }
    (None, None)
}

/// `error_recovery.build_agent_friendly_error(tool_name, error, arguments=...)`。
///
/// 工具抛异常时的入口:产出 canonical 错误信封(含兼容别名 `error_category`/`recovery_hint`,
/// 从 arguments 镜像标识字段 `file_path → symbol → query → roots[0] → mode`)。
/// `error_message` 形如 `"FileNotFoundError: File not found: x.py"`(类名 + 冒号 + str(exc))。
/// 对照 fixture `error_recovery.json` 每个用例的 `build_agent_friendly_error` 字段。
pub fn build_agent_friendly_error(
    tool_name: &str,
    error_message: &str,
    exc_class: &str,
    arguments: Option<&Value>,
) -> Value {
    // 消息文本优先分类,异常类仅作回退提示
    let classification = classify_with_hint(error_message, Some(exc_class));
    let (identifier_key, identifier_value) = pick_identifier(arguments);
    let summary_line = summary_line_for(tool_name, &classification.error_type, identifier_value.as_deref());
    // _build_envelope 的键序:success → error → error_type → error_category → recovery_hint
    // → verdict → agent_summary → summary_line → [suggested_tool] → [identifier]
    let mut body = Map::new();
    body.insert("success".to_string(), json!(false));
    body.insert("error".to_string(), json!(error_message));
    body.insert("error_type".to_string(), json!(classification.error_type));
    // 向后兼容的别名:老调用方固定读取此键名
    body.insert("error_category".to_string(), json!(classification.error_type));
    body.insert("recovery_hint".to_string(), json!(classification.recovery_hint));
    // Wave 1a:verdict 镜像到顶层,使错误路径与成功路径同样可读 result["verdict"]
    body.insert("verdict".to_string(), json!("ERROR"));
    let mut agent_summary = Map::new();
    agent_summary.insert("summary_line".to_string(), json!(summary_line));
    agent_summary.insert("next_step".to_string(), json!(classification.recovery_hint));
    agent_summary.insert("verdict".to_string(), json!("ERROR"));
    body.insert("agent_summary".to_string(), Value::Object(agent_summary));
    body.insert("summary_line".to_string(), json!(summary_line));
    if !classification.suggested_tool.is_empty() {
        body.insert("suggested_tool".to_string(), json!(classification.suggested_tool));
    }
    if let (Some(key), Some(value)) = (&identifier_key, &identifier_value) {
        body.insert(key.clone(), json!(value));
    }
    Value::Object(body)
}

/// `_first_error_text`:errors 列表首条消息;多条时聚合为 `"<n> errors; first: <msg>"`。
fn first_error_text(errors: Option<&Value>) -> Option<String> {
    let list = errors?.as_array()?;
    if list.is_empty() {
        return None;
    }
    let first = &list[0];
    let text: Option<&str> = if let Some(obj) = first.as_object() {
        ["error", "message", "reason"]
            .iter()
            .find_map(|key| obj.get(*key).and_then(Value::as_str).filter(|s| !s.is_empty()))
    } else {
        first.as_str().filter(|s| !s.is_empty())
    };
    let text = text?;
    if list.len() == 1 {
        Some(text.to_string())
    } else {
        Some(format!("{} errors; first: {text}", list.len()))
    }
}

/// `_derived_error_message`:生产者自己说过的话优先(message → errors → summary_line → status → 兜底)。
fn derived_error_message(response: &Map<String, Value>, tool_name: &str) -> String {
    if let Some(Value::String(message)) = response.get("message") {
        if !message.is_empty() {
            return message.clone();
        }
    }
    if let Some(listed) = first_error_text(response.get("errors")) {
        return listed;
    }
    let summary = response.get("agent_summary").and_then(Value::as_object);
    for candidate in [
        response.get("summary_line"),
        summary.and_then(|s| s.get("summary_line")),
    ] {
        if let Some(Value::String(s)) = candidate {
            if !s.is_empty() {
                return s.clone();
            }
        }
    }
    if let Some(Value::String(status)) = response.get("status") {
        if !status.is_empty() {
            return format!("{tool_name} failed with status {}", py_repr_str(status));
        }
    }
    format!("{tool_name} failed without reporting an error message")
}

/// `ensure_failure_identity`:给失败响应补 `error` 与机器码(已有者原样保留)。
fn ensure_failure_identity(response: &mut Map<String, Value>, tool_name: &str) {
    // error 缺失(None/空串)时从响应自身推导
    let error_missing = match response.get("error") {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(_) => false,
    };
    if error_missing {
        let derived = derived_error_message(response, tool_name);
        response.insert("error".to_string(), json!(derived));
    }
    let has_code = ["error_code", "error_type"].iter().any(|key| {
        response
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    });
    if !has_code {
        let error_text = response.get("error").map(py_str).unwrap_or_default();
        // 生产者常以自己的码开头("XXX: ...");该码优于猜测
        if let Some(code) = leading_error_code(&error_text) {
            response.insert("error_code".to_string(), json!(code));
        } else {
            let error_type = classify_with_hint(&error_text, None).error_type;
            response.insert("error_type".to_string(), json!(error_type));
        }
    }
}

/// `error_recovery.ensure_canonical_error_envelope(tool_name, payload, arguments=...)`。
///
/// 工具返回 `{success: False}` dict 时的补全入口:预设字段(`error_type`/`next_step`)不覆盖;
/// 缺 `error` 时从 `errors` 列表推导并从消息前导码推导 `error_code`。
/// 对照 fixture 的 `error_envelope_preserves_preset_fields`、`error_envelope_derives_identity`。
pub fn ensure_canonical_error_envelope(payload: &Value, tool_name: &str) -> Value {
    let Some(mut response) = payload.as_object().cloned() else {
        return payload.clone();
    };
    if response.get("success").and_then(Value::as_bool) != Some(false) {
        return Value::Object(response);
    }
    ensure_failure_identity(&mut response, tool_name);
    // error_msg = response.get("error") or response.get("message") or "Unknown error"
    let error_msg = match response
        .get("error")
        .filter(|v| py_truthy(v))
        .or_else(|| response.get("message").filter(|v| py_truthy(v)))
    {
        Some(value) => py_str(value),
        None => "Unknown error".to_string(),
    };
    let preset_error_type = response
        .get("error_type")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let classification = classify_with_hint(&error_msg, preset_error_type);
    let error_type = preset_error_type
        .map(str::to_string)
        .unwrap_or(classification.error_type.clone());
    let mut recovery_hint = classification.recovery_hint.clone();
    let suggested_tool = classification.suggested_tool.clone();
    // #1674:工具自己给出的 next_step 比通用提示更具体,不被 "Review the error message" 覆盖
    if let Some(Value::String(next_step)) = response.get("next_step") {
        if !next_step.is_empty() {
            recovery_hint = next_step.clone();
        }
    }
    // 标识字段:响应内已有的优先(顺序同 _IDENTIFIER_FIELDS);Rust 入口无 arguments 回退
    let mut identifier_key: Option<String> = None;
    let mut identifier_value: Option<String> = None;
    for key in IDENTIFIER_FIELDS {
        if let Some(Value::String(s)) = response.get(key) {
            if !s.is_empty() {
                identifier_key = Some(key.to_string());
                identifier_value = Some(s.clone());
                break;
            }
        }
        if key == "roots" {
            if let Some(Value::Array(list)) = response.get(key) {
                if let Some(Value::String(first)) = list.first() {
                    if !first.is_empty() {
                        identifier_key = Some("roots".to_string());
                        identifier_value = Some(first.clone());
                        break;
                    }
                }
            }
        }
    }
    // summary_line 缺失时合成:标签优先 error_code,其次 error_type
    if !response
        .get("summary_line")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        let label = response
            .get("error_code")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(error_type.as_str());
        let synthesized = summary_line_for(tool_name, label, identifier_value.as_deref());
        response.insert("summary_line".to_string(), json!(synthesized));
    }
    let summary_line = response
        .get("summary_line")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // setdefault 语义:预设字段不覆盖
    response
        .entry("error_type".to_string())
        .or_insert_with(|| json!(error_type.clone()));
    response
        .entry("error_category".to_string())
        .or_insert_with(|| json!(error_type.clone()));
    response
        .entry("recovery_hint".to_string())
        .or_insert_with(|| json!(recovery_hint.clone()));
    if !suggested_tool.is_empty() {
        response
            .entry("suggested_tool".to_string())
            .or_insert_with(|| json!(suggested_tool.clone()));
    }
    if let (Some(key), Some(value)) = (&identifier_key, &identifier_value) {
        response
            .entry(key.clone())
            .or_insert_with(|| json!(value.clone()));
    }
    // agent_summary 补全;verdict 双侧一致(优先真值,占位 "n/a" 视为缺失,兜底 ERROR)
    let mut agent_summary = response
        .get("agent_summary")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    agent_summary
        .entry("summary_line".to_string())
        .or_insert_with(|| json!(summary_line.clone()));
    agent_summary
        .entry("next_step".to_string())
        .or_insert_with(|| json!(recovery_hint.clone()));
    let verdict = real_verdict(response.get("verdict"))
        .or_else(|| real_verdict(agent_summary.get("verdict")))
        .unwrap_or("ERROR")
        .to_string();
    response.insert("verdict".to_string(), json!(verdict.clone()));
    agent_summary.insert("verdict".to_string(), json!(verdict));
    response.insert("agent_summary".to_string(), Value::Object(agent_summary));
    Value::Object(response)
}

// ---------- 成功路径补全(K12 / M14 / M10) ----------

/// K12 路径回显归一化(`_normalize_path_string`):`\` → `/`,循环剥掉前导 `./`。
///
/// 使 `./X` 与 `X` 字节一致;对照 fixture `k12_path_normalization.normalize_path_string_direct`
/// (`"./a\b.py"` → `"a/b.py"`)。
pub fn normalize_path_k12(path: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    let mut normalized = path.replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    normalized
}

/// K12:原位归一化回显的 `file_path` / `source_file`。
fn normalize_echoed_file_path(response: &mut Map<String, Value>) {
    for key in FILE_PATH_ECHO_KEYS {
        if let Some(Value::String(s)) = response.get(key) {
            if !s.is_empty() {
                let normalized = normalize_path_k12(s);
                if normalized != *s {
                    response.insert(key.to_string(), json!(normalized));
                }
            }
        }
    }
}

/// `Path(file).suffix` 的等价计算(小写;最后一个 `.` 且不在名字首位)。
fn path_suffix_lower(file_path: &str) -> String {
    let name = file_path.rsplit('/').next().unwrap_or("");
    match name.rfind('.') {
        Some(pos) if pos > 0 => name[pos..].to_lowercase(),
        _ => String::new(),
    }
}

/// `LanguageDetector.EXTENSION_MAPPING`:扩展名 → 语言名(小写)。
fn extension_language(extension: &str) -> Option<&'static str> {
    Some(match extension {
        // Java 系
        ".java" => "java",
        ".jsp" | ".jspx" => "jsp",
        // JavaScript / TypeScript 系
        ".js" | ".mjs" | ".cjs" => "javascript",
        ".jsx" => "jsx",
        ".ts" | ".tsx" | ".mts" | ".cts" => "typescript",
        // Python 系
        ".py" | ".pyx" | ".pyi" | ".pyw" => "python",
        // C / C++ 系
        ".c" => "c",
        ".cpp" | ".cxx" | ".cc" => "cpp",
        ".h" => "c",
        ".hpp" | ".hxx" => "cpp",
        // 其余语言
        ".rs" => "rust",
        ".go" => "go",
        ".rb" => "ruby",
        ".php" => "php",
        // Shell 脚本(BashPlugin 以 bash 文法解析 .zsh)
        ".sh" | ".bash" | ".zsh" => "bash",
        ".kt" | ".kts" => "kotlin",
        ".swift" | ".swiftinterface" => "swift",
        ".cs" => "csharp",
        ".vb" => "vbnet",
        ".fs" => "fsharp",
        ".scala" => "scala",
        ".clj" => "clojure",
        ".hs" => "haskell",
        ".ml" => "ocaml",
        ".lua" => "lua",
        ".pl" => "perl",
        ".r" => "r",
        ".m" => "objc",
        ".dart" => "dart",
        ".elm" => "elm",
        // Markdown 系
        ".md" | ".markdown" | ".mdown" | ".mkd" | ".mkdn" | ".mdx" => "markdown",
        // HTML 系
        ".html" | ".htm" | ".xhtml" => "html",
        // CSS 系
        ".css" | ".scss" | ".sass" | ".less" => "css",
        // SQL 系
        ".sql" => "sql",
        // JSON 系
        ".json" | ".jsonc" | ".json5" => "json",
        // YAML 系
        ".yaml" | ".yml" => "yaml",
        _ => return None,
    })
}

/// `detect_language_from_file`:扩展名检测语言;未知/失败返回 `None`(即 Python 侧 "unknown")。
fn detect_language_from_file(file_path: &str) -> Option<String> {
    if file_path.is_empty() {
        return None;
    }
    let extension = path_suffix_lower(file_path);
    let language = extension_language(&extension)?;
    if language.trim().is_empty() {
        return None;
    }
    Some(language.to_string())
}

/// M14:单文件响应回显 `language: <detected>`。
///
/// 跳过规则:已有非空且非 unknown 的 language;无 file_path 可探测(回退 arguments 参数);
/// 检测结果为 unknown(保持键缺席,调用方可按 `language is None` 分支)。
fn ensure_language_echo(response: &mut Map<String, Value>, file_path_arg: Option<&str>) {
    if let Some(existing) = response.get("language").and_then(Value::as_str) {
        if !existing.is_empty() && existing != "unknown" {
            return;
        }
    }
    let mut path: Option<&str> = response
        .get("file_path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if path.is_none() {
        path = file_path_arg.filter(|s| !s.is_empty());
    }
    let Some(path) = path else { return };
    // 语言检测尽力而为;失败不写键
    if let Some(detected) = detect_language_from_file(path) {
        response.insert("language".to_string(), json!(detected));
    }
}

/// `_verdict_suffix`:无标识时 summary_line 的兜底后缀。
///
/// success 类(或缺失)verdict 为 "ok";其余取 verdict 小写并将 `_` 换空格(NOT_FOUND → "not found")。
fn verdict_suffix(response: &Map<String, Value>) -> String {
    let mut verdict = response
        .get("verdict")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if verdict.is_none() {
        verdict = response
            .get("agent_summary")
            .and_then(Value::as_object)
            .and_then(|summary| summary.get("verdict"))
            .and_then(Value::as_str);
    }
    match verdict {
        Some(v) if !SUCCESS_LIKE_VERDICTS.contains(&v) => v.to_lowercase().replace('_', " "),
        _ => "ok".to_string(),
    }
}

/// `_populate_summary_line`:镜像 agent_summary.summary_line → 顶层,双双缺失时按标识合成。
fn populate_summary_line(
    response: &mut Map<String, Value>,
    tool_name: &str,
    file_path_arg: Option<&str>,
) -> String {
    let current = response
        .get("summary_line")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if current.is_none() {
        let candidate = response
            .get("agent_summary")
            .and_then(Value::as_object)
            .and_then(|summary| summary.get("summary_line"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if let Some(candidate) = candidate {
            response.insert("summary_line".to_string(), json!(candidate.clone()));
            return candidate;
        }
    }
    if let Some(current) = current {
        return current.to_string();
    }
    // 从响应内(仅 str 值)找最有信息量的标识;再回退请求参数的 file_path
    let mut identifier_value: Option<String> = None;
    for key in IDENTIFIER_FIELDS {
        if let Some(Value::String(s)) = response.get(key) {
            if !s.is_empty() {
                identifier_value = Some(s.clone());
                break;
            }
        }
    }
    if identifier_value.is_none() {
        if let Some(file_path) = file_path_arg.filter(|s| !s.is_empty()) {
            identifier_value = Some(file_path.to_string());
        }
    }
    // #672:兜底后缀必须反映 verdict —— WARN/ERROR 上说 "ok" 会误导按 summary_line 分支的 agent
    let synthesized = match identifier_value {
        Some(identifier) => format!("{tool_name}: {identifier}"),
        None => format!("{tool_name}: {}", verdict_suffix(response)),
    };
    response.insert("summary_line".to_string(), json!(synthesized));
    synthesized
}

/// `_populate_agent_summary_block`:确保 agent_summary 为 dict 且镜像 summary_line / next_step。
fn populate_agent_summary_block(
    response: &mut Map<String, Value>,
    summary_line: &str,
) -> Map<String, Value> {
    let mut agent_summary = response
        .get("agent_summary")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    agent_summary
        .entry("summary_line".to_string())
        .or_insert_with(|| json!(summary_line));
    // Wave 1b:顶层 next_step 有值而 agent_summary.next_step 缺失时镜像过去
    if !agent_summary.get("next_step").is_some_and(py_truthy) {
        let top_next = response
            .get("next_step")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        agent_summary.insert("next_step".to_string(), json!(top_next.unwrap_or("")));
    }
    response.insert("agent_summary".to_string(), Value::Object(agent_summary.clone()));
    agent_summary
}

/// `_mirror_verdict`(M10):顶层与 agent_summary 的 verdict 单侧设置时互相镜像;
/// 占位 `"n/a"` 视为未设置;双侧真值不同则保留(有意分歧)。
fn mirror_verdict(response: &mut Map<String, Value>, agent_summary: &mut Map<String, Value>) {
    let top = real_verdict(response.get("verdict"));
    let agent = real_verdict(agent_summary.get("verdict"));
    if top.is_some() && agent.is_none() {
        agent_summary.insert("verdict".to_string(), json!(top));
    } else if agent.is_some() && top.is_none() {
        response.insert("verdict".to_string(), json!(agent));
    }
}

/// `error_recovery.ensure_canonical_success_envelope(tool_name, payload)`。
///
/// 成功路径补全:K12(回显 `file_path`/`source_file` 归一化)、M14(单文件响应回显
/// `language`)、M10(verdict 顶层与 agent_summary 镜像,兜底 `agent_summary.verdict = "n/a"`)。
/// 对照 fixture 的 `k12_path_normalization` 与 `m14_language_echo`。
pub fn ensure_canonical_success_envelope(
    payload: &Value,
    tool_name: &str,
    file_path: Option<&str>,
) -> Value {
    let Some(mut response) = payload.as_object().cloned() else {
        return payload.clone();
    };
    if response.get("success").and_then(Value::as_bool) == Some(false) {
        // 错误路径走 ensure_canonical_error_envelope
        return Value::Object(response);
    }
    // K12:./X 与 X 字节一致(下游去重/缓存/展示)
    normalize_echoed_file_path(&mut response);
    // M14:单文件工具回显 language
    ensure_language_echo(&mut response, file_path);
    let summary_line_value = populate_summary_line(&mut response, tool_name, file_path);
    let mut agent_summary = populate_agent_summary_block(&mut response, &summary_line_value);
    // M10:verdict 顶层与 agent_summary 镜像
    mirror_verdict(&mut response, &mut agent_summary);
    // 最终兜底:两侧都没设 verdict 时填 "n/a" 保持信封形状稳定
    agent_summary
        .entry("verdict".to_string())
        .or_insert_with(|| json!("n/a"));
    response.insert("agent_summary".to_string(), Value::Object(agent_summary));
    Value::Object(response)
}

/// CLI 结构化输出(`--structure --output-format json` 路径)。
///
/// 信封契约:顶层含 `success`/`summary_line`/`verdict`/`agent_summary` 4 键
/// (`_CLI_ENVELOPE_KEYS`),`agent_summary` 含 `summary_line`/`verdict` 2 键,
/// 顶层 `verdict` 必须镜像 `agent_summary.verdict`。
/// 对照 fixture `cli/cli_outputs.json` 的 `structure` case(语料:`tests/golden/corpus_python.py`)。
pub fn cli_structure_json(file_path: &str) -> Result<Value, String> {
    todo!("Stage 2-3: cli_structure_json requires python_plugin AST extraction (Unit 3)")
}
