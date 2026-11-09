# Serafind for Fedora (COPR). The program is the one built, tested and published by the release
# workflow of the GitHub repository: Fedora's Rust is older than the version Serafind needs.
# The version is set from Cargo.toml when the source package is made (.copr/Makefile).

# A prebuilt, stripped binary: no debug information to extract.
%global debug_package %{nil}
%global app_id io.github.bluelabl_team.Serafind

Name:           serafind
Version:        0.1.1
Release:        1%{?dist}
Summary:        Quick search bar for files, folders, applications and text inside files

# Serafind itself, and the crates and fonts built into the program (THIRD_PARTY_LICENSES.html).
License:        Apache-2.0 AND MIT AND BSD-2-Clause AND BSD-3-Clause AND ISC AND Zlib AND Unicode-3.0 AND MPL-2.0 AND BSL-1.0 AND CC0-1.0 AND Unlicense AND 0BSD AND OFL-1.1 AND Ubuntu-font-1.0
URL:            https://github.com/BlueLabl-Team/Serafind
Source0:        %{url}/releases/download/v%{version}/serafind-%{version}-linux-x86_64.tar.gz
Source1:        %{url}/raw/v%{version}/crates/serafind/assets/%{app_id}.desktop
Source2:        %{url}/raw/v%{version}/crates/serafind/assets/logo/serafind.svg
Source3:        %{url}/raw/v%{version}/crates/serafind/assets/logo/serafind-256.png

ExclusiveArch:  x86_64
BuildRequires:  desktop-file-utils
Requires:       hicolor-icon-theme
# The global shortcut on Wayland and opening files go through the desktop portals.
Recommends:     xdg-desktop-portal

%description
Serafind is a quick search bar. Press Super+F anywhere, type, and open what you
were looking for: an application, a folder, a file, or a line of text inside a
file. A light resident process keeps an index of the home folder up to date.

%prep
%setup -q -n serafind-%{version}

%build
# Nothing to build: the program comes built from the release.

%install
install -Dpm 0755 serafind %{buildroot}%{_bindir}/serafind
install -Dpm 0644 %{SOURCE1} %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dpm 0644 %{SOURCE2} %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
install -Dpm 0644 %{SOURCE3} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/%{app_id}.png

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop
%{buildroot}%{_bindir}/serafind --version

%files
%license LICENSE THIRD_PARTY_LICENSES.html Inter-LICENSE.txt Phosphor-LICENSE.txt
%doc README.md
%{_bindir}/serafind
%{_datadir}/applications/%{app_id}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
%{_datadir}/icons/hicolor/256x256/apps/%{app_id}.png

%changelog
* Mon Nov 09 2026 BlueLabL <https://github.com/BlueLabl-Team/Serafind> - 0.1.1-1
- First package: see https://github.com/BlueLabl-Team/Serafind/releases
