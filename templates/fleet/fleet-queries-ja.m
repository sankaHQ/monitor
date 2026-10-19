section Section1;

shared FolderPath = "C:\SankaMonitor" meta [IsParameterQuery = true, Type = "Text", IsParameterQueryRequired = true];

shared Records = let
    Columns = {"record", "report_id", "scanned_at", "hostname", "serial_number", "device_model", "platform", "os_version", "os_edition", "agent_version", "catalog_verified_on", "check_id", "check_pass", "detail", "software_source", "software_identifier", "software_name", "software_version", "software_publisher", "finding_type", "area", "severity", "lifecycle_rule", "lifecycle_status", "support_ends_on", "successor", "source_url", "title_en", "title_ja", "fix_en", "fix_ja"},
    Files = Folder.Files(FolderPath),
    Reports = Table.SelectRows(Files, each Text.Lower([Extension]) = ".csv" and not Text.StartsWith([Name], "~$")),
    Parsed = Table.AddColumn(Reports, "Rows", each
        let
            Csv = Csv.Document([Content], [Delimiter = ",", Encoding = 65001, QuoteStyle = QuoteStyle.Csv]),
            Promoted = Table.PromoteHeaders(Csv, [PromoteAllScalars = true]),
            First = Table.ColumnNames(Promoted){0},
            Clean = if First = "record" then Promoted else Table.RenameColumns(Promoted, {{First, Text.Replace(First, Character.FromNumber(65279), "")}})
        in
            Clean),
    Ours = Table.SelectRows(Parsed, each List.ContainsAll(Table.ColumnNames([Rows]), {"record", "report_id", "hostname"})),
    Combined = Table.Combine(List.Combine({{#table(Columns, {})}, Ours[Rows]})),
    Typed = Table.SelectColumns(Combined, Columns, MissingField.UseNull)
in
    Typed;

shared Devices = let
    Rows = Records,
    Device = Table.SelectColumns(Table.SelectRows(Rows, each [record] = "device"), {"hostname", "serial_number", "device_model", "os_version", "os_edition", "lifecycle_status", "support_ends_on", "scanned_at", "agent_version"}),
    Findings = Table.SelectRows(Rows, each [record] = "finding"),
    Count = (rows as table, level as text) as number => Table.RowCount(Table.SelectRows(rows, each [severity] = level)),
    Counts = Table.Group(Findings, {"hostname", "serial_number"}, {{"high", each Count(_, "high"), Int64.Type}, {"medium", each Count(_, "medium"), Int64.Type}, {"low", each Count(_, "low"), Int64.Type}}),
    CheckRows = Table.SelectColumns(Table.SelectRows(Rows, each [record] = "check"), {"hostname", "serial_number", "check_id", "check_pass"}),
    Pivoted = Table.Pivot(CheckRows, {"disk_encryption", "firewall", "antivirus", "screen_lock", "os_update", "automatic_updates", "local_admin", "device_management", "gatekeeper", "secure_boot", "system_integrity"}, "check_id", "check_pass", List.First),
    WithCounts = Table.ExpandTableColumn(Table.NestedJoin(Device, {"hostname", "serial_number"}, Counts, {"hostname", "serial_number"}, "Counts", JoinKind.LeftOuter), "Counts", {"high", "medium", "low"}),
    WithChecks = Table.ExpandTableColumn(Table.NestedJoin(WithCounts, {"hostname", "serial_number"}, Pivoted, {"hostname", "serial_number"}, "Checks", JoinKind.LeftOuter), "Checks", {"disk_encryption", "firewall", "antivirus", "screen_lock", "os_update", "automatic_updates", "local_admin", "device_management", "gatekeeper", "secure_boot", "system_integrity"}),
    Zeroed = Table.ReplaceValue(WithChecks, null, 0, Replacer.ReplaceValue, {"high", "medium", "low"}),
    Labeled = Table.TransformColumns(Zeroed, {{"lifecycle_status", each if _ = "ended" then "サポート終了" else if _ = "ending" then "サポート終了間近" else if _ = "supported" then "サポート中" else "未判定", type text}, {"disk_encryption", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"firewall", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"antivirus", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"screen_lock", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"os_update", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"automatic_updates", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"local_admin", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"device_management", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"gatekeeper", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"secure_boot", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}, {"system_integrity", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}}),
    Sorted = Table.Sort(Labeled, {{"high", Order.Descending}, {"medium", Order.Descending}, {"hostname", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "serial_number", "device_model", "os_version", "os_edition", "lifecycle_status", "support_ends_on", "high", "medium", "low", "disk_encryption", "firewall", "antivirus", "screen_lock", "os_update", "automatic_updates", "local_admin", "device_management", "gatekeeper", "secure_boot", "system_integrity", "scanned_at", "agent_version"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "コンピューター名"}, {"serial_number", "シリアル番号"}, {"device_model", "機種"}, {"os_version", "OS"}, {"os_edition", "エディション"}, {"lifecycle_status", "OSサポート状況"}, {"support_ends_on", "OSサポート最終日"}, {"high", "高"}, {"medium", "中"}, {"low", "低"}, {"disk_encryption", "ディスク暗号化"}, {"firewall", "ファイアウォール有効"}, {"antivirus", "ウイルス対策有効"}, {"screen_lock", "画面ロック設定済み"}, {"os_update", "OS 最新"}, {"automatic_updates", "自動アップデート有効"}, {"local_admin", "管理者権限なし"}, {"device_management", "管理対象（MDM）"}, {"gatekeeper", "Gatekeeper / SmartScreen 有効"}, {"secure_boot", "セキュアブート有効"}, {"system_integrity", "SIP 有効"}, {"scanned_at", "スキャン日時"}, {"agent_version", "Sanka Monitor バージョン"}})
in
    Named;

shared Software = let
    Rows = Table.SelectRows(Records, each [record] = "software"),
    Labeled = Table.TransformColumns(Rows, {{"lifecycle_status", each if _ = "ended" then "サポート終了" else if _ = "ending" then "サポート終了間近" else if _ = "supported" then "サポート中" else "未判定", type text}}),
    Sorted = Table.Sort(Labeled, {{"software_name", Order.Ascending}, {"hostname", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "software_name", "software_version", "software_publisher", "lifecycle_status", "support_ends_on", "successor", "software_source", "software_identifier", "source_url"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "コンピューター名"}, {"software_name", "名称"}, {"software_version", "バージョン"}, {"software_publisher", "提供元"}, {"lifecycle_status", "サポート状況"}, {"support_ends_on", "サポート最終日"}, {"successor", "推奨する対応"}, {"software_source", "種別"}, {"software_identifier", "識別子"}, {"source_url", "提供元の情報"}})
in
    Named;

shared Risks = let
    Rows = Table.SelectRows(Records, each [record] = "finding"),
    Ranked = Table.AddColumn(Rows, "rank", each if [severity] = "high" then 1 else if [severity] = "medium" then 2 else 3, Int64.Type),
    Sorted = Table.Sort(Ranked, {{"rank", Order.Ascending}, {"hostname", Order.Ascending}}),
    Labeled = Table.TransformColumns(Sorted, {{"severity", each if _ = "high" then "高" else if _ = "medium" then "中" else if _ = "low" then "低" else "", type text}, {"area", each if _ = "device" then "デバイス" else if _ = "software" then "ソフトウェア" else "", type text}}),
    Ordered = Table.SelectColumns(Labeled, {"severity", "area", "title_ja", "hostname", "detail", "fix_ja", "support_ends_on", "source_url"}),
    Named = Table.RenameColumns(Ordered, {{"severity", "重要度"}, {"area", "対象"}, {"title_ja", "リスク"}, {"hostname", "コンピューター名"}, {"detail", "根拠"}, {"fix_ja", "対処方法"}, {"support_ends_on", "サポート最終日"}, {"source_url", "提供元の情報"}})
in
    Named;

shared Checks = let
    Rows = Table.SelectRows(Records, each [record] = "check"),
    Labeled = Table.TransformColumns(Rows, {{"check_pass", each if _ = "true" then "合格" else if _ = "false" then "不合格" else "判定不可", type text}}),
    Sorted = Table.Sort(Labeled, {{"hostname", Order.Ascending}, {"check_id", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "check_id", "check_pass", "detail", "scanned_at"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "コンピューター名"}, {"check_id", "チェック項目"}, {"check_pass", "結果"}, {"detail", "詳細"}, {"scanned_at", "スキャン日時"}})
in
    Named;
