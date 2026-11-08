%global debug_package %{nil}

Name:           runa
Version:        0.15.2
Release:        1%{?dist}
Summary:        A fast and lightweight terminal file manager written in Rust

License:        MIT OR Apache-2.0
URL:            https://github.com/runa-fm/runa
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-vendor.tar.gz

ExclusiveArch:  x86_64 aarch64
BuildRequires:  cargo >= 1.90
BuildRequires:  rust >= 1.90

%description
runa is a fast, keyboard-focused terminal file manager (TUI).
Highly configurable and lightweight.

%prep
%autosetup -n %{name}-%{version}
tar -xzf %{SOURCE1}
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
cargo build --frozen --release

%check
cargo test --frozen

%install
install -Dm755 target/release/rn -t %{buildroot}%{_bindir}

%files
%license LICENSE-MIT LICENSE-APACHE
%doc README.md docs/config/configuration.md docs/config/config-reference
%{_bindir}/rn

%changelog
* Sun Nov 08 2026 Alexandros McCray <alexandros@amccray.com> - 0.15.2-1
- Initial package
