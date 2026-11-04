"""Writes data/data.xlsx, the sample workbook the tests import.

The values are a deliberate pattern, not measurements: 1, 2, 3, 4, 5 for the four
constructs, 0 for the two controls, and the same rows again as formulas times ten.
Tests assert those numbers exactly, and the formula cells cover reading cached
values out of a workbook.

    python scripts/make-sample-data.py
"""

import zipfile
from pathlib import Path

COLUMNS = ["Well", "Condition", "Additive", "IL2", "IL7", "IL15", "IL18", "IL21"]

# Two wells per condition: four constructs that respond, two controls that do not.
CONDITIONS = [
    ("Construct A", [1, 2, 3, 4, 5]),
    ("Construct B", [1, 2, 3, 4, 5]),
    ("Construct C", [1, 2, 3, 4, 5]),
    ("Construct D", [1, 2, 3, 4, 5]),
    ("Mock", [0, 0, 0, 0, 0]),
    ("WT", [0, 0, 0, 0, 0]),
]
# The additive each pair of wells got in the second plate half.
ADDITIVES = ["IL12", "IL12", "IL23", "IL23", "IL23", "IL23", "Carrier", "Carrier", "IL12", "IL12", "IL2", "IL2"]


def rows():
    """(well, condition, additive, values) for all 24 rows, plate A then plate B."""
    pairs = [(name, values) for name, values in CONDITIONS for _ in range(2)]
    for i, (name, values) in enumerate(pairs):
        yield f"A{i + 1}", name, None, values
    for i, (name, values) in enumerate(pairs):
        yield f"B{i + 1}", name, ADDITIVES[i], values


def main():
    strings, index = [], {}

    def shared(text):
        if text not in index:
            index[text] = len(strings)
            strings.append(text)
        return index[text]

    def col(n):
        return chr(ord("A") + n)

    header = "".join(f'<c r="{col(n)}1" t="s"><v>{shared(name)}</v></c>' for n, name in enumerate(COLUMNS))
    cells = [(1, header)]

    for r, (well, condition, additive, values) in enumerate(rows(), start=2):
        row = [f'<c r="A{r}" t="s"><v>{shared(well)}</v></c>', f'<c r="B{r}" t="s"><v>{shared(condition)}</v></c>']
        # Plate A has no additive at all: the cell is absent, so the column's first
        # level is the blank one, which is what the import test checks.
        if additive is not None:
            row.append(f'<c r="C{r}" t="s"><v>{shared(additive)}</v></c>')
        for n, value in enumerate(values, start=3):
            if additive is None:
                row.append(f'<c r="{col(n)}{r}"><v>{value}</v></c>')
            else:
                # Plate B is plate A times ten, written as a formula with its cached
                # value, so importing has to read the cache rather than evaluate.
                source = f"{col(n)}{r - 12}"
                row.append(f'<c r="{col(n)}{r}"><f>{source}*10</f><v>{value * 10}</v></c>')
        cells.append((r, "".join(row)))

    sheet = "".join(f'<row r="{r}">{c}</row>' for r, c in cells)
    sheet_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f'<dimension ref="A1:H{len(cells)}"/><sheetData>{sheet}</sheetData></worksheet>'
    )
    shared_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{len(strings)}" uniqueCount="{len(strings)}">'
        + "".join(f"<si><t>{s}</t></si>" for s in strings)
        + "</sst>"
    )
    parts = {
        "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        "</Types>",
        "_rels/.rels": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        "</Relationships>",
        "xl/workbook.xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>',
        "xl/_rels/workbook.xml.rels": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>'
        '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        "</Relationships>",
        "xl/styles.xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        '<fills count="1"><fill><patternFill patternType="none"/></fill></fills>'
        '<borders count="1"><border/></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
        "</styleSheet>",
        "xl/worksheets/sheet1.xml": sheet_xml,
        "xl/sharedStrings.xml": shared_xml,
    }

    out = Path(__file__).resolve().parent.parent / "data" / "data.xlsx"
    # A fixed timestamp for every entry, so rebuilding gives a byte-identical file
    # and the workbook carries no author or date of its own.
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for name, text in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 10, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, text)
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
