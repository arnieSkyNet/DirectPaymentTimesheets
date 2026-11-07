"""Compile the actual Stage 1 owner/storage code; never execute generated snippets.

Linux-only local check. No native/terminal fixtures or production data are used.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    if sys.platform != "linux":
        raise RuntimeError("Stage 1 borrowing check is Linux-only")
    root = Path(__file__).resolve().parents[1]
    build = subprocess.run(
        ["cargo", "test", "--locked", "--offline", "--bin",
         "direct_payment_timesheets", "--no-run", "--message-format=json"],
        cwd=root, capture_output=True, text=True, timeout=300, check=True,
    )
    records = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    libraries = {}
    for name in ("rusqlite", "libc", "tempfile", "same_file"):
        paths = {path for record in records
                 if record.get("reason") == "compiler-artifact"
                 and record.get("target", {}).get("name") == name
                 for path in record["filenames"] if path.endswith(".rlib")}
        if len(paths) != 1:
            raise RuntimeError("dependency artifact inventory mismatch: " + name)
        libraries[name] = paths.pop()
    prefix = "".join(
        '#[path = ' + json.dumps(str(root / "src" / (name + ".rs"))) + '] mod ' + name + ';\n'
        for name in ("private_storage", "database_failure", "database_connection")
    ) + "use database_connection::DatabaseConnection;\n"
    cases = [
        ("positive_borrowing", None, """
fn borrow(owner: &DatabaseConnection) {
    let statement = owner.connection().prepare("SELECT 1").unwrap();
    drop(statement);
}
fn transaction(owner: &mut DatabaseConnection) { owner.transaction().unwrap().rollback().unwrap(); }
fn close(owner: DatabaseConnection) { let _complete_owner_on_failure = owner.close(); }
fn backup(source: &DatabaseConnection, destination: &mut DatabaseConnection) {
    let copy = source.backup_into(destination).unwrap();
    drop(copy);
}
"""),
        ("private_connection", "E0616", "fn escape(owner: &mut DatabaseConnection) { owner.connection.take(); }"),
        ("private_resources", "E0616", "fn escape(owner: &mut DatabaseConnection) { let _ = &mut owner.resources; }"),
        ("no_mutable_escape", "E0599", "fn escape(owner: &mut DatabaseConnection) { owner.connection_mut(); }"),
        ("no_raw_constructor", "E0624", "fn escape(db: rusqlite::Connection) { DatabaseConnection::from_parts(db, vec![], None); }"),
        ("statement_blocks_close", "E0505", """
fn close(owner: DatabaseConnection) {
    let statement = owner.connection().prepare("SELECT 1").unwrap();
    let _ = owner.close();
    drop(statement);
}
"""),
        ("transaction_blocks_close", "E0505", """
fn close(mut owner: DatabaseConnection) {
    let tx = owner.transaction().unwrap();
    let _ = owner.close();
    drop(tx);
}
"""),
        ("backup_blocks_source_close", "E0505", """
fn close(source: DatabaseConnection, mut destination: DatabaseConnection) {
    let backup = source.backup_into(&mut destination).unwrap();
    let _ = source.close();
    drop(backup);
}
"""),
        ("backup_blocks_destination_close", "E0505", """
fn close(source: DatabaseConnection, mut destination: DatabaseConnection) {
    let backup = source.backup_into(&mut destination).unwrap();
    let _ = destination.close();
    drop(backup);
}
"""),
    ]
    with tempfile.TemporaryDirectory(prefix="dpt-stage1-api-") as directory:
        directory = Path(directory)
        for name, expected, snippet in cases:
            source = directory / (name + ".rs")
            source.write_text(prefix + snippet)
            command = ["rustc", "--edition=2021", "--crate-type=lib", "--emit=metadata",
                       "--error-format=json", "--cap-lints=allow", "-L",
                       "dependency=" + str(Path(libraries["rusqlite"]).parent),
                       "--out-dir", str(directory)]
            for library, path in libraries.items():
                command.extend(["--extern", library + "=" + path])
            result = subprocess.run(command + [str(source)], cwd=root,
                                    capture_output=True, text=True, timeout=30)
            diagnostics = [json.loads(line) for line in result.stderr.splitlines() if line.startswith("{")]
            errors = [item for item in diagnostics if item.get("level") == "error"]
            codes = {item["code"]["code"] for item in errors if item.get("code")}
            if expected is None:
                if result.returncode != 0 or errors:
                    raise RuntimeError(name + " failed: " + result.stderr)
            elif result.returncode == 0 or codes != {expected}:
                raise RuntimeError(name + " had unrelated/missing compiler errors: " + result.stderr)
            print(name + ": PASS" + (" (" + expected + ")" if expected else ""))


if __name__ == "__main__":
    main()
