import os
import re

replacements = {
    r"\baimaxxing_core\b": "brain",
    r"\baimaxxing_providers\b": "providers",
    r"\baimaxxing_engram\b": "engram",
    r"\baimaxxing-core\b": "brain",
    r"\baimaxxing-providers\b": "providers",
    r"\baimaxxing-engram\b": "engram"
}

def replace_in_file(filepath):
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
    except UnicodeDecodeError:
        return

    original = content
    for old, new in replacements.items():
        content = re.sub(old, new, content)

    if content != original:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Updated {filepath}")

for root, dirs, files in os.walk("."):
    if "target" in root.split(os.sep) or ".git" in root.split(os.sep):
        continue
    for file in files:
        if file.endswith(".rs") or file.endswith(".toml") or file.endswith(".md"):
            replace_in_file(os.path.join(root, file))

print("Done.")

