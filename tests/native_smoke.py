"""Small disposable CLI lifecycle check; never accesses actual agent directories."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="uncle-smoke-") as temporary:
    root = Path(temporary)
    repo = root / "source repo"
    source = repo / "skills" / "smoke-skill"
    source.mkdir(parents=True)
    (source / "SKILL.md").write_text('---\nname: smoke-skill\ndescription: "Test a disposable installation."\n---\nKeep original files.\n')
    destination = root / "custom-agent" / "skills"
    config = root / "settings" / "config.toml"
    def run(*args, success=True):
        result = subprocess.run([str(binary), "--config", str(config), *map(str,args)],
                                capture_output=True, text=True)
        assert result.returncode == 0 if success else result.returncode != 0, result.stderr
        return result
    method = "copy" if os.name == "nt" else "symlink"
    run("init", "--repo", repo, "--target", "claude-code="+str(destination), "--method", method)
    run("sync", "--dry-run")
    assert not destination.exists()
    destination.joinpath("synced").mkdir(parents=True)
    destination.joinpath("synced/keep").write_text("agent-owned")
    run("sync")
    deployed = destination / source.name
    assert (deployed / "SKILL.md").read_bytes() == (source / "SKILL.md").read_bytes()
    run("sync")
    run("doctor")
    (source / "SKILL.md").unlink()
    source.rmdir()
    run("sync")
    assert not os.path.lexists(deployed)
    assert (destination / "synced/keep").read_text() == "agent-owned"
    print("Native smoke passed: init, preview, install, repeat, doctor, removal, preserved synced")
