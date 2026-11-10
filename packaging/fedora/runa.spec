%bcond check 1

%global cargo_install_lib 0

Name:           runa
Version:        0.15.5
Release:        1%{?dist}
Summary:        Fast and lightweight terminal file manager

License:        (MIT OR Apache-2.0) AND Unicode-3.0 AND (Apache-2.0 OR BSL-1.0) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND CC0-1.0 AND ISC AND MIT AND (MIT OR Unlicense) AND Zlib AND (Zlib OR Apache-2.0 OR MIT)
URL:            https://github.com/runa-fm/runa
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-vendor.tar.xz

BuildRequires:  cargo-rpm-macros

%description
runa is a fast, keyboard-focused terminal file manager (TUI).
Highly configurable and lightweight.

%prep
%autosetup -n %{name}-%{version} -p1 -a1
%cargo_prep -v vendor

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}

%install
%cargo_install

%if %{with check}
%check
%cargo_test
%endif

%files
%license LICENSE-MIT LICENSE-APACHE
%license LICENSE.dependencies
%license cargo-vendor.txt
%doc README.md
%doc docs/config/configuration.md
%doc docs/config/config-reference
%{_bindir}/rn

%changelog
* Tue Nov 09 2026 Alexandros McCray <alexandros@amccray.com> - 0.15.5-1
- Update to 0.15.5

* Mon Nov 09 2026 Alexandros McCray <alexandros@amccray.com> - 0.15.4-1
- Update to 0.15.4

* Sun Nov 08 2026 Alexandros McCray <alexandros@amccray.com> - 0.15.3-1
- Initial package
