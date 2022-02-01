%define __spec_install_post %{nil}
%define __os_install_post %{_dbpath}/brp-compress
%define debug_package %{nil}

Name: zenoh-router
Summary: The zenoh router
Version: @@VERSION@@
Release: @@RELEASE@@%{?dist}
License:  EPL-2.0 OR Apache-2.0
Group: System Environment/Daemons
Source0: %{name}-%{version}.tar.gz
URL: http://zenoh.io

BuildRoot: %{_tmppath}/%{name}-%{version}-%{release}-root

%description
%{summary}

%prep
%setup -q

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a * %{buildroot}

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
