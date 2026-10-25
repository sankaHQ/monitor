#!/usr/bin/env python3
"""Builds the Sanka Monitor fleet workbooks: Excel files whose Power Query
reads a folder of per-PC CSV files (written by `sanka-monitor-agent scan
--out <folder>`) and merges them into one IT asset ledger.

    python3 templates/fleet/build_fleet_template.py [--folder <path>] [--out <dir>]

writes, next to this script (or to --out):
- sanka-monitor-fleet-en.xlsx and sanka-monitor-fleet-ja.xlsx: the
  templates (How to use, Devices, Software, SaaS, Risks, Checks);
- fleet-queries-en.m and fleet-queries-ja.m: the same Power Query code as
  text, for anyone who prefers to build the queries by hand.

Standard library only, and deterministic: the same script writes the same
bytes, so a change to a template shows up as a change to this script.

The queries live in the workbook's DataMashup part (customXml/item1.xml, the
format described in Microsoft's [MS-QDEFF]); each loaded query has a
connection, a query table and a worksheet table, as Excel writes them. The
folder is the `FolderPath` parameter. Nothing in the workbook runs until the
person refreshes it in Excel. `--folder` presets the folder, for an IT team
that hands out the workbook with its own share path.
"""

from __future__ import annotations

import argparse
import base64
import io
import struct
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
FIXED_TIME = (2026, 10, 19, 0, 0, 0)
DEFAULT_FOLDER = r"C:\SankaMonitor"

# The per-PC CSV columns (monitor-report `csv::COLUMNS`).
CSV_COLUMNS = [
    "record", "report_id", "scanned_at", "hostname", "serial_number", "device_model",
    "platform", "os_version", "os_edition", "agent_version", "catalog_verified_on",
    "check_id", "check_pass", "detail", "software_source", "software_identifier",
    "software_name", "software_version", "software_publisher", "finding_type", "area",
    "severity", "lifecycle_rule", "lifecycle_status", "support_ends_on", "successor",
    "source_url", "title_en", "title_ja", "fix_en", "fix_ja",
    "saas_key", "saas_name", "saas_vendor", "saas_category", "saas_ai", "saas_source",
    "saas_desktop_apps", "saas_last_used_on", "saas_days_used_30", "saas_allowed",
    "sanka_migrate_url", "sanka_connector", "browser_check", "allowlist",
]

# The SaaS catalog's categories (monitor-saas `Category`), as the reports name them.
CATEGORIES = {
    "en": {
        "ai": "AI", "chat": "Chat", "meetings": "Meetings and video",
        "suite": "Office suite and email", "files": "File sharing", "crm": "CRM and sales",
        "marketing": "Marketing", "design": "Design and whiteboards",
        "support": "Customer support", "itsm": "IT service and device management",
        "work": "Project and work management", "knowledge": "Docs and notes",
        "dev": "Developer tools", "cloud": "Cloud platforms", "analytics": "Analytics and BI",
        "automation": "Automation", "hr": "HR and recruiting",
        "finance": "Accounting and expenses", "erp": "ERP", "esign": "E-signature",
        "commerce": "E-commerce", "identity": "Sign-in and identity", "security": "Security",
        "remote": "Remote access",
    },
    "ja": {
        "ai": "AI", "chat": "チャット", "meetings": "会議・動画",
        "suite": "グループウェア・メール", "files": "ファイル共有", "crm": "CRM・営業",
        "marketing": "マーケティング", "design": "デザイン・ホワイトボード",
        "support": "カスタマーサポート", "itsm": "ITサービス・デバイス管理",
        "work": "プロジェクト・タスク管理", "knowledge": "ドキュメント・ノート",
        "dev": "開発ツール", "cloud": "クラウド基盤", "analytics": "分析・BI",
        "automation": "自動化", "hr": "人事・採用", "finance": "会計・経費", "erp": "ERP",
        "esign": "電子契約", "commerce": "EC", "identity": "認証・ID管理",
        "security": "セキュリティ", "remote": "リモートアクセス",
    },
}

CHECK_IDS = [
    "disk_encryption", "firewall", "antivirus", "screen_lock", "os_update",
    "automatic_updates", "local_admin", "device_management", "gatekeeper",
    "secure_boot", "system_integrity",
]

TEXT = {
    "en": {
        "pass": "Pass", "fail": "Fail", "unknown": "Unknown",
        "high": "High", "medium": "Medium", "low": "Low",
        "area_device": "Device", "area_software": "Software", "area_saas": "SaaS",
        "ended": "Support ended", "ending": "Support ending soon",
        "supported": "Supported", "not_assessed": "Not assessed",
        "allowed": "Allowed", "not_allowed": "Not allowed", "no_list": "No company list",
        "desktop_app": "Desktop app", "browser": "Browser", "both": "Desktop app and browser",
        "title": "title_en", "fix": "fix_en",
        "sheets": ["How to use", "Devices", "Software", "SaaS", "Risks", "Checks"],
        "devices": [
            ("hostname", "Computer name"), ("serial_number", "Serial number"),
            ("device_model", "Model"), ("os_version", "OS"), ("os_edition", "Edition"),
            ("lifecycle_status", "OS support"), ("support_ends_on", "OS last day of support"),
            ("high", "High"), ("medium", "Medium"), ("low", "Low"),
            ("disk_encryption", "Disk encrypted"), ("firewall", "Firewall on"),
            ("antivirus", "Antivirus on"), ("screen_lock", "Screen lock set"),
            ("os_update", "OS up to date"), ("automatic_updates", "Automatic updates on"),
            ("local_admin", "No local admin rights"), ("device_management", "Managed (MDM)"),
            ("gatekeeper", "Gatekeeper / SmartScreen on"), ("secure_boot", "Secure Boot on"),
            ("system_integrity", "SIP on"), ("scanned_at", "Scanned at"),
            ("agent_version", "Sanka Monitor version"),
        ],
        "software": [
            ("hostname", "Computer name"), ("software_name", "Name"),
            ("software_version", "Version"), ("software_publisher", "Publisher"),
            ("lifecycle_status", "Support status"), ("support_ends_on", "Last day of support"),
            ("successor", "Recommended action"), ("software_source", "Type"),
            ("software_identifier", "Identifier"), ("source_url", "Vendor source"),
        ],
        "saas": [
            ("hostname", "Computer name"), ("saas_name", "SaaS"), ("saas_vendor", "Vendor"),
            ("saas_category", "Category"), ("saas_ai", "AI"), ("saas_source", "Found from"),
            ("saas_desktop_apps", "Desktop app"), ("saas_last_used_on", "Last used (browser)"),
            ("saas_days_used_30", "Days used in the last 30 (browser)"),
            ("saas_allowed", "Company allows it"), ("sanka_migrate_url", "Sanka migration page"),
            ("sanka_connector", "Sanka connector"),
        ],
        "risks": [
            ("severity", "Severity"), ("area", "Area"), ("title", "Risk"),
            ("hostname", "Computer name"), ("detail", "Evidence"), ("fix", "How to fix"),
            ("support_ends_on", "Last day of support"), ("source_url", "Vendor source"),
        ],
        "checks": [
            ("hostname", "Computer name"), ("check_id", "Check"), ("check_pass", "Result"),
            ("detail", "Detail"), ("scanned_at", "Scanned at"),
        ],
        "guide": [
            "Sanka Monitor fleet ledger",
            "",
            "This workbook merges the reports that the Sanka Monitor command-line scan writes, one CSV file per PC, from one folder.",
            "",
            "1. Run the scan on each PC from your device management tool, writing to one shared folder:",
            r"     sanka-monitor-agent scan --out \\fileserver\SankaMonitor",
            "2. Set the folder: Data > Get Data > Launch Power Query Editor, select FolderPath in the Queries list, and type the folder path as its Current Value.",
            "3. Close & Load, then Data > Refresh All. Refresh again whenever new scans arrive. (Data > Refresh updates only the sheet you are on.)",
            "If Excel shows a security bar saying external data connections are disabled, click Enable Content first.",
            "On a Mac, type the folder as a path such as /Users/you/SankaMonitor/. Excel asks once for access to it: click Select..., then Grant Access.",
            "",
            "Devices, Software, SaaS, Risks and Checks fill from the folder. A PC appears once: a new scan replaces its file.",
            "SaaS come from each PC's installed desktop apps. With a company allowlist (sanka-monitor-agent scan --allowlist <file>), SaaS that are not on it are listed as risks.",
            "Nothing in this workbook connects to Sanka. If Excel asks about privacy levels, choose Organizational for the folder.",
            "To get your risks reviewed, send this workbook to your Sanka contact.",
        ],
    },
    "ja": {
        "pass": "合格", "fail": "不合格", "unknown": "判定不可",
        "high": "高", "medium": "中", "low": "低",
        "area_device": "デバイス", "area_software": "ソフトウェア", "area_saas": "SaaS",
        "ended": "サポート終了", "ending": "サポート終了間近",
        "supported": "サポート中", "not_assessed": "未判定",
        "allowed": "許可済み", "not_allowed": "未許可", "no_list": "許可リストなし",
        "desktop_app": "デスクトップアプリ", "browser": "ブラウザー", "both": "デスクトップアプリ、ブラウザー",
        "title": "title_ja", "fix": "fix_ja",
        "sheets": ["使い方", "端末台帳", "ソフトウェア台帳", "SaaS台帳", "リスク一覧", "セキュリティチェック"],
        "devices": [
            ("hostname", "コンピューター名"), ("serial_number", "シリアル番号"),
            ("device_model", "機種"), ("os_version", "OS"), ("os_edition", "エディション"),
            ("lifecycle_status", "OSサポート状況"), ("support_ends_on", "OSサポート最終日"),
            ("high", "高"), ("medium", "中"), ("low", "低"),
            ("disk_encryption", "ディスク暗号化"), ("firewall", "ファイアウォール有効"),
            ("antivirus", "ウイルス対策有効"), ("screen_lock", "画面ロック設定済み"),
            ("os_update", "OS 最新"), ("automatic_updates", "自動アップデート有効"),
            ("local_admin", "管理者権限なし"), ("device_management", "管理対象（MDM）"),
            ("gatekeeper", "Gatekeeper / SmartScreen 有効"), ("secure_boot", "セキュアブート有効"),
            ("system_integrity", "SIP 有効"), ("scanned_at", "スキャン日時"),
            ("agent_version", "Sanka Monitor バージョン"),
        ],
        "software": [
            ("hostname", "コンピューター名"), ("software_name", "名称"),
            ("software_version", "バージョン"), ("software_publisher", "提供元"),
            ("lifecycle_status", "サポート状況"), ("support_ends_on", "サポート最終日"),
            ("successor", "推奨する対応"), ("software_source", "種別"),
            ("software_identifier", "識別子"), ("source_url", "提供元の情報"),
        ],
        "saas": [
            ("hostname", "コンピューター名"), ("saas_name", "SaaS"), ("saas_vendor", "提供元"),
            ("saas_category", "カテゴリ"), ("saas_ai", "AI"), ("saas_source", "検出元"),
            ("saas_desktop_apps", "デスクトップアプリ"), ("saas_last_used_on", "最終利用日（ブラウザー）"),
            ("saas_days_used_30", "直近30日の利用日数（ブラウザー）"),
            ("saas_allowed", "会社の許可"), ("sanka_migrate_url", "Sankaの移行ページ"),
            ("sanka_connector", "Sankaのコネクタ"),
        ],
        "risks": [
            ("severity", "重要度"), ("area", "対象"), ("title", "リスク"),
            ("hostname", "コンピューター名"), ("detail", "根拠"), ("fix", "対処方法"),
            ("support_ends_on", "サポート最終日"), ("source_url", "提供元の情報"),
        ],
        "checks": [
            ("hostname", "コンピューター名"), ("check_id", "チェック項目"), ("check_pass", "結果"),
            ("detail", "詳細"), ("scanned_at", "スキャン日時"),
        ],
        "guide": [
            "Sanka Monitor 台帳（複数台）",
            "",
            "このブックは、Sanka Monitor のコマンドラインスキャンが PC ごとに書き出す CSV ファイルを、1 つのフォルダーからまとめて読み込みます。",
            "",
            "1. デバイス管理ツールから各 PC でスキャンを実行し、共有フォルダーに書き出します。",
            r"     sanka-monitor-agent scan --out \\fileserver\SankaMonitor --lang ja",
            "2. フォルダーを設定します。[データ] > [データの取得] > [Power Query エディターの起動] で、クエリ一覧の FolderPath を選び、[現在の値] にフォルダーのパスを入力してください。",
            "3. [閉じて読み込む] を選び、[データ] > [すべて更新] を実行します。新しいスキャンが届いたら、もう一度更新してください（[データ] > [更新] は表示中のシートだけを更新します）。",
            "「外部データ接続が無効になっています」というセキュリティの警告が表示されたら、先に [コンテンツの有効化] をクリックしてください。",
            "Mac では、フォルダーを /Users/ユーザー名/SankaMonitor/ のようなパスで入力します。初回だけフォルダーへのアクセス許可を求められるので、[選択...] をクリックしてから [アクセス権を付与] をクリックしてください。",
            "",
            "端末台帳、ソフトウェア台帳、SaaS台帳、リスク一覧、セキュリティチェックにフォルダーの内容が反映されます。同じ PC は 1 行です（新しいスキャンでファイルが置き換わります）。",
            "SaaS は各 PC のインストール済みデスクトップアプリから検出します。会社の許可リストを指定すると（sanka-monitor-agent scan --allowlist <ファイル>）、一覧にない SaaS をリスクとして表示します。",
            "このブックは Sanka に接続しません。プライバシー レベルを尋ねられたら、フォルダーに [組織] を選んでください。",
            "リスクの確認を依頼するときは、このブックを Sanka の担当者に送ってください。",
        ],
    },
}


def m_text(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def m_list(values: list[str]) -> str:
    return "{" + ", ".join(m_text(v) for v in values) + "}"


def m_renames(pairs: list[tuple[str, str]], text: dict) -> str:
    resolved = [(text.get(src, src) if src in ("title", "fix") else src, dst) for src, dst in pairs]
    return "{" + ", ".join("{" + m_text(s) + ", " + m_text(d) + "}" for s, d in resolved) + "}"


def m_label(column: str, mapping: dict[str, str], default: str) -> str:
    cases = " else ".join(f"if _ = {m_text(k)} then {m_text(v)}" for k, v in mapping.items())
    return f"{{{m_text(column)}, each {cases} else {m_text(default)}, type text}}"


def queries(lang: str, folder: str) -> str:
    t = TEXT[lang]
    status = {"ended": t["ended"], "ending": t["ending"], "supported": t["supported"]}
    severity = {"high": t["high"], "medium": t["medium"], "low": t["low"]}
    passes = {"true": t["pass"], "false": t["fail"]}
    device_cols = [src for src, _ in t["devices"] if src not in ("high", "medium", "low", *CHECK_IDS)]
    title, fix = t["title"], t["fix"]
    check_labels = ", ".join(m_label(c, passes, t["unknown"]) for c in CHECK_IDS)
    return f"""section Section1;

shared FolderPath = {m_text(folder)} meta [IsParameterQuery = true, Type = "Text", IsParameterQueryRequired = true];

shared Records = let
    Columns = {m_list(CSV_COLUMNS)},
    Files = Folder.Files(FolderPath),
    Reports = Table.SelectRows(Files, each Text.Lower([Extension]) = ".csv" and not Text.StartsWith([Name], "~$")),
    Parsed = Table.AddColumn(Reports, "Rows", each
        let
            Csv = Csv.Document([Content], [Delimiter = ",", Encoding = 65001, QuoteStyle = QuoteStyle.Csv]),
            Promoted = Table.PromoteHeaders(Csv, [PromoteAllScalars = true]),
            First = Table.ColumnNames(Promoted){{0}},
            Clean = if First = "record" then Promoted else Table.RenameColumns(Promoted, {{{{First, Text.Replace(First, Character.FromNumber(65279), "")}}}})
        in
            Clean),
    Ours = Table.SelectRows(Parsed, each List.ContainsAll(Table.ColumnNames([Rows]), {{"record", "report_id", "hostname"}})),
    Combined = Table.Combine(List.Combine({{{{#table(Columns, {{}})}}, Ours[Rows]}})),
    Typed = Table.SelectColumns(Combined, Columns, MissingField.UseNull)
in
    Typed;

shared Devices = let
    Rows = Records,
    Device = Table.SelectColumns(Table.SelectRows(Rows, each [record] = "device"), {m_list(device_cols)}),
    Findings = Table.SelectRows(Rows, each [record] = "finding"),
    Count = (rows as table, level as text) as number => Table.RowCount(Table.SelectRows(rows, each [severity] = level)),
    Counts = Table.Group(Findings, {{"hostname", "serial_number"}}, {{{{"high", each Count(_, "high"), Int64.Type}}, {{"medium", each Count(_, "medium"), Int64.Type}}, {{"low", each Count(_, "low"), Int64.Type}}}}),
    CheckRows = Table.SelectColumns(Table.SelectRows(Rows, each [record] = "check"), {{"hostname", "serial_number", "check_id", "check_pass"}}),
    Pivoted = Table.Pivot(CheckRows, {m_list(CHECK_IDS)}, "check_id", "check_pass", List.First),
    WithCounts = Table.ExpandTableColumn(Table.NestedJoin(Device, {{"hostname", "serial_number"}}, Counts, {{"hostname", "serial_number"}}, "Counts", JoinKind.LeftOuter), "Counts", {{"high", "medium", "low"}}),
    WithChecks = Table.ExpandTableColumn(Table.NestedJoin(WithCounts, {{"hostname", "serial_number"}}, Pivoted, {{"hostname", "serial_number"}}, "Checks", JoinKind.LeftOuter), "Checks", {m_list(CHECK_IDS)}),
    Zeroed = Table.ReplaceValue(WithChecks, null, 0, Replacer.ReplaceValue, {{"high", "medium", "low"}}),
    Labeled = Table.TransformColumns(Zeroed, {{{m_label("lifecycle_status", status, t["not_assessed"])}, {check_labels}}}),
    Sorted = Table.Sort(Labeled, {{{{"high", Order.Descending}}, {{"medium", Order.Descending}}, {{"hostname", Order.Ascending}}}}),
    Ordered = Table.SelectColumns(Sorted, {m_list([src for src, _ in t["devices"]])}),
    Named = Table.RenameColumns(Ordered, {m_renames(t["devices"], t)})
in
    Named;

shared Software = let
    Rows = Table.SelectRows(Records, each [record] = "software"),
    Labeled = Table.TransformColumns(Rows, {{{m_label("lifecycle_status", status, t["not_assessed"])}}}),
    Sorted = Table.Sort(Labeled, {{{{"software_name", Order.Ascending}}, {{"hostname", Order.Ascending}}}}),
    Ordered = Table.SelectColumns(Sorted, {m_list([src for src, _ in t["software"]])}),
    Named = Table.RenameColumns(Ordered, {m_renames(t["software"], t)})
in
    Named;

shared SaaS = let
    Rows = Table.SelectRows(Records, each [record] = "saas"),
    Labeled = Table.TransformColumns(Rows, {{{m_label("saas_allowed", {"true": t["allowed"], "false": t["not_allowed"]}, t["no_list"])}, {m_label("saas_ai", {"true": "AI"}, "")}, {m_label("saas_source", {"desktop_app": t["desktop_app"], "browser": t["browser"], "desktop_app,browser": t["both"]}, "")}, {m_label("saas_category", CATEGORIES[lang], "")}}}),
    Sorted = Table.Sort(Labeled, {{{{"saas_name", Order.Ascending}}, {{"hostname", Order.Ascending}}}}),
    Ordered = Table.SelectColumns(Sorted, {m_list([src for src, _ in t["saas"]])}),
    Named = Table.RenameColumns(Ordered, {m_renames(t["saas"], t)})
in
    Named;

shared Risks = let
    Rows = Table.SelectRows(Records, each [record] = "finding"),
    Ranked = Table.AddColumn(Rows, "rank", each if [severity] = "high" then 1 else if [severity] = "medium" then 2 else 3, Int64.Type),
    Sorted = Table.Sort(Ranked, {{{{"rank", Order.Ascending}}, {{"hostname", Order.Ascending}}}}),
    Labeled = Table.TransformColumns(Sorted, {{{m_label("severity", severity, "")}, {m_label("area", {"device": t["area_device"], "software": t["area_software"], "saas": t["area_saas"]}, "")}}}),
    Ordered = Table.SelectColumns(Labeled, {m_list([title if s == "title" else fix if s == "fix" else s for s, _ in t["risks"]])}),
    Named = Table.RenameColumns(Ordered, {m_renames(t["risks"], t)})
in
    Named;

shared Checks = let
    Rows = Table.SelectRows(Records, each [record] = "check"),
    Labeled = Table.TransformColumns(Rows, {{{m_label("check_pass", passes, t["unknown"])}}}),
    Sorted = Table.Sort(Labeled, {{{{"hostname", Order.Ascending}}, {{"check_id", Order.Ascending}}}}),
    Ordered = Table.SelectColumns(Sorted, {m_list([src for src, _ in t["checks"]])}),
    Named = Table.RenameColumns(Ordered, {m_renames(t["checks"], t)})
in
    Named;
"""


# --- DataMashup ([MS-QDEFF]) -------------------------------------------------

def zip_bytes(files: list[tuple[str, bytes]]) -> bytes:
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in files:
            info = zipfile.ZipInfo(name, FIXED_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    return buffer.getvalue()


def metadata_item(path: str, entries: list[tuple[str, str]]) -> str:
    stable = "".join(f'<Entry Type="{k}" Value="{escape(v, {chr(34): "&quot;"})}" />' for k, v in entries)
    item_type = "AllFormulas" if path == "" else "Formula"
    item_path = "<ItemPath />" if path == "" else f"<ItemPath>{escape(path)}</ItemPath>"
    return (
        f"<Item><ItemLocation><ItemType>{item_type}</ItemType>{item_path}</ItemLocation>"
        f"<StableEntries>{stable}</StableEntries></Item>"
    )


def data_mashup(m_code: str, loaded: dict[str, list[str]]) -> bytes:
    package = zip_bytes([
        ("[Content_Types].xml", (
            '<?xml version="1.0" encoding="utf-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="xml" ContentType="text/xml" /><Default Extension="m" ContentType="application/x-ms-m" /></Types>'
        ).encode("utf-8")),
        ("Config/Package.xml", (
            '<?xml version="1.0" encoding="utf-8"?><Package xmlns:xsd="http://www.w3.org/2001/XMLSchema" '
            'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><Version>2.138.0.0</Version>'
            '<MinVersion>2.21.0.0</MinVersion><Culture>en-US</Culture></Package>'
        ).encode("utf-8")),
        ("Formulas/Section1.m", m_code.encode("utf-8")),
    ])
    permissions = (
        '\ufeff<?xml version="1.0" encoding="utf-8"?><PermissionList xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" '
        'xmlns:xsd="http://www.w3.org/2001/XMLSchema"><CanEvaluateFuturePackages>false</CanEvaluateFuturePackages>'
        '<FirewallEnabled>true</FirewallEnabled><WorkbookGroupType xsi:nil="true" /></PermissionList>'
    ).encode("utf-8")
    items = [metadata_item("", [])]
    items.append(metadata_item("Section1/FolderPath", [
        ("IsPrivate", "l0"), ("FillEnabled", "l0"), ("FillObjectType", "sConnectionOnly"),
        ("FillToDataModelEnabled", "l0"), ("ResultType", "sText"), ("BufferNextRefresh", "l1"),
    ]))
    items.append(metadata_item("Section1/Records", [
        ("IsPrivate", "l0"), ("FillEnabled", "l0"), ("FillObjectType", "sConnectionOnly"),
        ("FillToDataModelEnabled", "l0"), ("ResultType", "sTable"), ("BufferNextRefresh", "l1"),
    ]))
    for name, columns in loaded.items():
        names = "[" + ",".join('"' + c.replace('"', '\\"') + '"' for c in columns) + "]"
        items.append(metadata_item(f"Section1/{name}", [
            ("IsPrivate", "l0"), ("FillEnabled", "l1"), ("FillObjectType", "sTable"),
            ("FillToDataModelEnabled", "l0"), ("ResultType", "sTable"), ("BufferNextRefresh", "l1"),
            ("FillTarget", "s" + name), ("FilledCompleteResultToWorksheet", "l1"),
            ("AddedToDataModel", "l0"), ("FillCount", "l0"), ("FillErrorCode", "sUnknown"),
            ("FillErrorCount", "l0"), ("FillColumnNames", "s" + names), ("FillStatus", "sComplete"),
            ("NameUpdatedAfterFill", "l0"),
        ]))
    metadata_xml = (
        '\ufeff<?xml version="1.0" encoding="utf-8"?><LocalPackageMetadataFile xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" '
        'xmlns:xsd="http://www.w3.org/2001/XMLSchema"><Items>' + "".join(items) + "</Items></LocalPackageMetadataFile>"
    ).encode("utf-8")
    metadata_content = zip_bytes([("[Content_Types].xml", (
        '<?xml version="1.0" encoding="utf-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types" />'
    ).encode("utf-8"))])
    metadata = struct.pack("<I", 0) + struct.pack("<I", len(metadata_xml)) + metadata_xml + struct.pack("<I", len(metadata_content)) + metadata_content
    # Permission bindings are a DPAPI signature only Excel can make; without
    # them Excel applies its default permissions, which is what we want.
    return (
        struct.pack("<I", 0)
        + struct.pack("<I", len(package)) + package
        + struct.pack("<I", len(permissions)) + permissions
        + struct.pack("<I", len(metadata)) + metadata
        + struct.pack("<I", 0)
    )


# --- Workbook ----------------------------------------------------------------

def col_letter(index: int) -> str:
    letters = ""
    index += 1
    while index:
        index, rem = divmod(index - 1, 26)
        letters = chr(65 + rem) + letters
    return letters


def cell(ref: str, text: str, style: int = 0) -> str:
    return f'<c r="{ref}" t="inlineStr" s="{style}"><is><t xml:space="preserve">{escape(text)}</t></is></c>'


def table_sheet(headers: list[str], width_hint: list[int], table_rel: str) -> str:
    cols = "".join(f'<col min="{i + 1}" max="{i + 1}" width="{w}" customWidth="1"/>' for i, w in enumerate(width_hint))
    header = "".join(cell(f"{col_letter(i)}1", h, 1) for i, h in enumerate(headers))
    empty = "".join(f'<c r="{col_letter(i)}2" s="0"/>' for i in range(len(headers)))
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>'
        f'<sheetFormatPr defaultRowHeight="15"/><cols>{cols}</cols>'
        f'<sheetData><row r="1">{header}</row><row r="2">{empty}</row></sheetData>'
        f'<tableParts count="1"><tablePart r:id="{table_rel}"/></tableParts></worksheet>'
    )


def guide_sheet(lines: list[str]) -> str:
    rows = "".join(
        f'<row r="{i + 1}">{cell(f"A{i + 1}", line, 2 if i == 0 else 3)}</row>' for i, line in enumerate(lines)
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<sheetViews><sheetView workbookViewId="0" showGridLines="0"/></sheetViews>'
        '<sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="1" width="140" customWidth="1"/></cols>'
        f"<sheetData>{rows}</sheetData></worksheet>"
    )


STYLES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
    '<fonts count="3"><font><sz val="10"/><name val="Meiryo UI"/></font>'
    '<font><b/><sz val="10"/><name val="Meiryo UI"/></font>'
    '<font><b/><sz val="16"/><name val="Meiryo UI"/></font></fonts>'
    '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
    '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
    '<cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
    '<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>'
    '<xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyFont="1"/>'
    '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1" vertical="top"/></xf></cellXfs>'
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
    "</styleSheet>"
)


def build(lang: str, folder: str = DEFAULT_FOLDER) -> tuple[bytes, str]:
    t = TEXT[lang]
    m_code = queries(lang, folder)
    tables = [
        ("Devices", [h for _, h in t["devices"]], [18, 14, 16, 30, 12, 14, 14, 6, 6, 6] + [12] * 11 + [20, 12]),
        ("Software", [h for _, h in t["software"]], [18, 40, 16, 26, 16, 14, 34, 14, 30, 30]),
        ("SaaS", [h for _, h in t["saas"]], [18, 30, 18, 22, 6, 18, 26, 14, 12, 14, 40, 16]),
        ("Risks", [h for _, h in t["risks"]], [8, 10, 44, 18, 44, 56, 14, 30]),
        ("Checks", [h for _, h in t["checks"]], [18, 20, 10, 50, 20]),
    ]
    files: list[tuple[str, bytes]] = []
    sheet_names = t["sheets"]
    overrides = [
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>',
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>',
        '<Override PartName="/xl/connections.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml"/>',
        '<Override PartName="/customXml/itemProps1.xml" ContentType="application/vnd.openxmlformats-officedocument.customXmlProperties+xml"/>',
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>',
    ]
    files.append(("xl/worksheets/sheet1.xml", guide_sheet(t["guide"]).encode("utf-8")))
    connections = []
    defined_names = []
    for index, (name, headers, widths) in enumerate(tables, start=1):
        sheet_no = index + 1
        ref = f"A1:{col_letter(len(headers) - 1)}2"
        files.append((f"xl/worksheets/sheet{sheet_no}.xml", table_sheet(headers, widths, "rId1").encode("utf-8")))
        files.append((f"xl/worksheets/_rels/sheet{sheet_no}.xml.rels", (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table{index}.xml"/></Relationships>'
        ).encode("utf-8")))
        columns = "".join(
            f'<tableColumn id="{i + 1}" uniqueName="{i + 1}" name="{escape(h, {chr(34): "&quot;"})}" queryTableFieldId="{i + 1}"/>'
            for i, h in enumerate(headers)
        )
        files.append((f"xl/tables/table{index}.xml", (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="{index}" name="{name}" displayName="{name}" '
            f'ref="{ref}" tableType="queryTable" totalsRowShown="0"><autoFilter ref="{ref}"/>'
            f'<tableColumns count="{len(headers)}">{columns}</tableColumns>'
            '<tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>'
        ).encode("utf-8")))
        files.append((f"xl/tables/_rels/table{index}.xml.rels", (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable{index}.xml"/></Relationships>'
        ).encode("utf-8")))
        fields = "".join(
            f'<queryTableField id="{i + 1}" name="{escape(h, {chr(34): "&quot;"})}" tableColumnId="{i + 1}"/>'
            for i, h in enumerate(headers)
        )
        files.append((f"xl/queryTables/queryTable{index}.xml", (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<queryTable xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="ExternalData_{index}" connectionId="{index}" '
            'autoFormatId="16" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" '
            'applyAlignmentFormats="0" applyWidthHeightFormats="0">'
            f'<queryTableRefresh nextId="{len(headers) + 1}"><queryTableFields count="{len(headers)}">{fields}</queryTableFields></queryTableRefresh></queryTable>'
        ).encode("utf-8")))
        connections.append(
            f'<connection id="{index}" keepAlive="1" name="Query - {name}" description="Connection to the &apos;{name}&apos; query in the workbook." '
            'type="5" refreshedVersion="8" background="1" saveData="1">'
            f'<dbPr connection="Provider=Microsoft.Mashup.OleDb.1;Data Source=$Workbook$;Location={name};Extended Properties=&quot;&quot;" '
            f'command="SELECT * FROM [{name}]"/></connection>'
        )
        defined_names.append(
            f'<definedName name="ExternalData_{index}" localSheetId="{index}" hidden="1">'
            f"&apos;{escape(sheet_names[index])}&apos;!${col_letter(0)}$1:${col_letter(len(headers) - 1)}$2</definedName>"
        )
        overrides += [
            f'<Override PartName="/xl/worksheets/sheet{sheet_no}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>',
            f'<Override PartName="/xl/tables/table{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml"/>',
            f'<Override PartName="/xl/queryTables/queryTable{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml"/>',
        ]
    sheets = "".join(
        f'<sheet name="{escape(n)}" sheetId="{i + 1}" r:id="rId{i + 1}"/>' for i, n in enumerate(sheet_names)
    )
    files.append(("xl/workbook.xml", (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f'<bookViews><workbookView activeTab="0"/></bookViews><sheets>{sheets}</sheets>'
        f'<definedNames>{"".join(defined_names)}</definedNames><calcPr calcId="191029"/></workbook>'
    ).encode("utf-8")))
    rels = "".join(
        f'<Relationship Id="rId{i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i + 1}.xml"/>'
        for i in range(len(sheet_names))
    )
    n = len(sheet_names)
    rels += (
        f'<Relationship Id="rId{n + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        f'<Relationship Id="rId{n + 2}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/>'
        f'<Relationship Id="rId{n + 3}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/>'
    )
    files.append(("xl/_rels/workbook.xml.rels", (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f"{rels}</Relationships>"
    ).encode("utf-8")))
    files.append(("xl/styles.xml", STYLES.encode("utf-8")))
    files.append(("xl/connections.xml", (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><connections xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        + "".join(connections) + "</connections>"
    ).encode("utf-8")))
    loaded = {name: headers for name, headers, _ in tables}
    mashup = base64.b64encode(data_mashup(m_code, loaded)).decode("ascii")
    item = '<?xml version="1.0" encoding="utf-16"?><DataMashup xmlns="http://schemas.microsoft.com/DataMashup">' + mashup + "</DataMashup>"
    files.append(("customXml/item1.xml", b"\xff\xfe" + item.encode("utf-16-le")))
    files.append(("customXml/itemProps1.xml", (
        '<?xml version="1.0" encoding="UTF-8" standalone="no"?><ds:datastoreItem ds:itemID="{8C3E3A2B-5C2E-4F0B-9E43-7F1C5A0D9A61}" '
        'xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs/></ds:datastoreItem>'
    ).encode("utf-8")))
    files.append(("customXml/_rels/item1.xml.rels", (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps" Target="itemProps1.xml"/></Relationships>'
    ).encode("utf-8")))
    files.append(("_rels/.rels", (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'
    ).encode("utf-8")))
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>' + "".join(overrides) + "</Types>"
    )
    files.insert(0, ("[Content_Types].xml", content_types.encode("utf-8")))
    return zip_bytes(files), m_code


def main() -> None:
    parser = argparse.ArgumentParser(description="Build the Sanka Monitor fleet workbooks.")
    parser.add_argument("--folder", default=DEFAULT_FOLDER, help="the folder the queries read")
    parser.add_argument("--out", type=Path, default=HERE, help="where to write the files")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for lang in ("en", "ja"):
        workbook, m_code = build(lang, args.folder)
        (args.out / f"sanka-monitor-fleet-{lang}.xlsx").write_bytes(workbook)
        (args.out / f"fleet-queries-{lang}.m").write_text(m_code, encoding="utf-8")
        print(f"wrote {args.out}/sanka-monitor-fleet-{lang}.xlsx and fleet-queries-{lang}.m")


if __name__ == "__main__":
    main()
