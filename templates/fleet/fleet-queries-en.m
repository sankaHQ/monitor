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
    Labeled = Table.TransformColumns(Zeroed, {{"lifecycle_status", each if _ = "ended" then "Support ended" else if _ = "ending" then "Support ending soon" else if _ = "supported" then "Supported" else "Not assessed", type text}, {"disk_encryption", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"firewall", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"antivirus", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"screen_lock", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"os_update", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"automatic_updates", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"local_admin", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"device_management", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"gatekeeper", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"secure_boot", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}, {"system_integrity", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}}),
    Sorted = Table.Sort(Labeled, {{"high", Order.Descending}, {"medium", Order.Descending}, {"hostname", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "serial_number", "device_model", "os_version", "os_edition", "lifecycle_status", "support_ends_on", "high", "medium", "low", "disk_encryption", "firewall", "antivirus", "screen_lock", "os_update", "automatic_updates", "local_admin", "device_management", "gatekeeper", "secure_boot", "system_integrity", "scanned_at", "agent_version"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "Computer name"}, {"serial_number", "Serial number"}, {"device_model", "Model"}, {"os_version", "OS"}, {"os_edition", "Edition"}, {"lifecycle_status", "OS support"}, {"support_ends_on", "OS last day of support"}, {"high", "High"}, {"medium", "Medium"}, {"low", "Low"}, {"disk_encryption", "Disk encrypted"}, {"firewall", "Firewall on"}, {"antivirus", "Antivirus on"}, {"screen_lock", "Screen lock set"}, {"os_update", "OS up to date"}, {"automatic_updates", "Automatic updates on"}, {"local_admin", "No local admin rights"}, {"device_management", "Managed (MDM)"}, {"gatekeeper", "Gatekeeper / SmartScreen on"}, {"secure_boot", "Secure Boot on"}, {"system_integrity", "SIP on"}, {"scanned_at", "Scanned at"}, {"agent_version", "Sanka Monitor version"}})
in
    Named;

shared Software = let
    Rows = Table.SelectRows(Records, each [record] = "software"),
    Labeled = Table.TransformColumns(Rows, {{"lifecycle_status", each if _ = "ended" then "Support ended" else if _ = "ending" then "Support ending soon" else if _ = "supported" then "Supported" else "Not assessed", type text}}),
    Sorted = Table.Sort(Labeled, {{"software_name", Order.Ascending}, {"hostname", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "software_name", "software_version", "software_publisher", "lifecycle_status", "support_ends_on", "successor", "software_source", "software_identifier", "source_url"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "Computer name"}, {"software_name", "Name"}, {"software_version", "Version"}, {"software_publisher", "Publisher"}, {"lifecycle_status", "Support status"}, {"support_ends_on", "Last day of support"}, {"successor", "Recommended action"}, {"software_source", "Type"}, {"software_identifier", "Identifier"}, {"source_url", "Vendor source"}})
in
    Named;

shared Risks = let
    Rows = Table.SelectRows(Records, each [record] = "finding"),
    Ranked = Table.AddColumn(Rows, "rank", each if [severity] = "high" then 1 else if [severity] = "medium" then 2 else 3, Int64.Type),
    Sorted = Table.Sort(Ranked, {{"rank", Order.Ascending}, {"hostname", Order.Ascending}}),
    Labeled = Table.TransformColumns(Sorted, {{"severity", each if _ = "high" then "High" else if _ = "medium" then "Medium" else if _ = "low" then "Low" else "", type text}, {"area", each if _ = "device" then "Device" else if _ = "software" then "Software" else "", type text}}),
    Ordered = Table.SelectColumns(Labeled, {"severity", "area", "title_en", "hostname", "detail", "fix_en", "support_ends_on", "source_url"}),
    Named = Table.RenameColumns(Ordered, {{"severity", "Severity"}, {"area", "Area"}, {"title_en", "Risk"}, {"hostname", "Computer name"}, {"detail", "Evidence"}, {"fix_en", "How to fix"}, {"support_ends_on", "Last day of support"}, {"source_url", "Vendor source"}})
in
    Named;

shared Checks = let
    Rows = Table.SelectRows(Records, each [record] = "check"),
    Labeled = Table.TransformColumns(Rows, {{"check_pass", each if _ = "true" then "Pass" else if _ = "false" then "Fail" else "Unknown", type text}}),
    Sorted = Table.Sort(Labeled, {{"hostname", Order.Ascending}, {"check_id", Order.Ascending}}),
    Ordered = Table.SelectColumns(Sorted, {"hostname", "check_id", "check_pass", "detail", "scanned_at"}),
    Named = Table.RenameColumns(Ordered, {{"hostname", "Computer name"}, {"check_id", "Check"}, {"check_pass", "Result"}, {"detail", "Detail"}, {"scanned_at", "Scanned at"}})
in
    Named;
