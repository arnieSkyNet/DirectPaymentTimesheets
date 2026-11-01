"""Compile the actual test-only prototype's borrow/privacy contracts; never execute it."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    if sys.platform != "linux":
        raise RuntimeError("unsupported_platform: prototype is Linux-only")
    root = Path(__file__).resolve().parents[1]
    build = subprocess.run(
        ["cargo", "test", "--locked", "--offline", "--test",
         "guarded_connection_close_proof", "--no-run", "--message-format=json"],
        cwd=root, capture_output=True, text=True, timeout=180, check=True,
    )
    records = [json.loads(line) for line in build.stdout.splitlines()
               if line.startswith("{")]
    libraries = {}
    for name in ("rusqlite", "libc", "tempfile"):
        paths = {path for record in records
                 if record.get("reason") == "compiler-artifact"
                 and record.get("target", {}).get("name") == name
                 for path in record["filenames"] if path.endswith(".rlib")}
        if len(paths) != 1:
            raise RuntimeError("dependency_artifact_inventory_mismatch: " + name)
        libraries[name] = paths.pop()
    prototype = root / "tests/support/sqlite_owner_prototype.rs"
    prefix = (f"#[path = {json.dumps(str(prototype))}] mod prototype;\n"
              "use prototype::{ProofConnection, Staging};\n"
              "use rusqlite::Connection;\n")
    cases = [
        ("positive_borrows", None, """
fn statements(owner: &ProofConnection) {
    let statement = owner.db().prepare("SELECT 1").unwrap();
    drop(statement);
}
fn transactions(owner: &mut ProofConnection) {
    let transaction = owner.transaction().unwrap();
    transaction.rollback().unwrap();
}
fn backup(source: &ProofConnection, destination: &mut ProofConnection) {
    let copy = source.backup_into(destination).unwrap();
    drop(copy);
}
fn checked_close(owner: ProofConnection) {
    let _complete_owner_and_error = owner.close();
}
"""),
        ("raw_connection_field", "E0616", """
fn escape(mut owner: ProofConnection) -> Option<Connection> { owner.connection.take() }
"""),
        ("mutable_connection_escape", "E0599", """
fn escape(owner: &mut ProofConnection) -> &mut Connection { owner.db_mut() }
"""),
        ("detach_staging", "E0616", """
fn detach(owner: &mut ProofConnection, replacement: std::rc::Rc<Staging>) {
    std::mem::replace(&mut owner.staging, replacement);
}
"""),
        ("statement_blocks_close", "E0505", """
fn close(owner: ProofConnection) {
    let statement = owner.db().prepare("SELECT 1").unwrap();
    let _ = owner.close();
    drop(statement);
}
"""),
        ("transaction_blocks_close", "E0505", """
fn close(mut owner: ProofConnection) {
    let transaction = owner.transaction().unwrap();
    let _ = owner.close();
    drop(transaction);
}
"""),
        ("backup_blocks_source_close", "E0505", """
fn close(source: ProofConnection, mut destination: ProofConnection) {
    let copy = source.backup_into(&mut destination).unwrap();
    let _ = source.close();
    drop(copy);
}
"""),
        ("backup_blocks_destination_close", "E0505", """
fn close(source: ProofConnection, mut destination: ProofConnection) {
    let copy = source.backup_into(&mut destination).unwrap();
    let _ = destination.close();
    drop(copy);
}
"""),
    ]
    with tempfile.TemporaryDirectory(prefix="dpt-owner-api-") as directory:
        directory = Path(directory)
        for name, expected, snippet in cases:
            source = directory / (name + ".rs")
            source.write_text(prefix + snippet)
            command = ["rustc", "--edition=2021", "--crate-type=lib",
                       "--emit=metadata", "--error-format=json", "--cap-lints=allow",
                       "-L", "dependency=" + str(Path(libraries["rusqlite"]).parent),
                       "--out-dir", str(directory)]
            for library, path in libraries.items():
                command.extend(["--extern", library + "=" + path])
            result = subprocess.run(command + [str(source)], cwd=root,
                                    capture_output=True, text=True, timeout=30)
            diagnostics = [json.loads(line) for line in result.stderr.splitlines()
                           if line.startswith("{")]
            errors = [d for d in diagnostics if d.get("level") == "error"]
            codes = [d["code"]["code"] for d in errors if d.get("code")]
            if expected is None:
                assert result.returncode == 0 and not errors, name
            else:
                assert result.returncode != 0 and codes and set(codes) == {expected}, name
            print(name + ": PASS" + (" (" + expected + ")" if expected else ""))


if __name__ == "__main__":
    main()
