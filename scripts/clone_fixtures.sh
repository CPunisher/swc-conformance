# Function to clone a repository in the background
clone_repo() {
    local repo="$1"
    local path="$2"
    local ref="$3"
    local name="$4"

    echo "Starting clone of $name..."
    (
        # Create the directory structure if it doesn't exist
        mkdir -p "$(dirname "$path")"

        # Clone the repository with minimal progress output
        git clone --quiet --no-progress --single-branch --depth 1 \
            "https://github.com/$repo.git" "$path"

        # Checkout the specific commit
        cd "$path"
        git fetch --quiet --depth 1 origin "$ref"
        git checkout --quiet "$ref"

        echo "✓ Completed clone of $name"
    )
}

clone_repo "tc39/test262" "fixtures/test262" "d1d583db95a521218f3eb8341a887fd63eda8ff1" "test262"
clone_repo "microsoft/TypeScript" "fixtures/typescript" "637d5746b70257028fb95aad32ddec6b26ab0a14" "typescript"