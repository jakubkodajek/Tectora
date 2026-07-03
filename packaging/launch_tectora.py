"""PyInstaller entry point — a plain script wrapping the GUI main()."""
from tectora.gui.main import main

if __name__ == "__main__":
    raise SystemExit(main())
