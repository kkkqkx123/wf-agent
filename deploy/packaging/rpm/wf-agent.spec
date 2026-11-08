Name:           wf-agent
Version: 0.1.0
Release:        1%{?dist}
Summary:        wf-agent workflow and autonomous agent framework
License:        Apache-2.0
BuildArch:      x86_64
Requires:       systemd
BuildRequires:  systemd-rpm-macros

%description
Modular agent framework unifying graph-based workflow orchestration
with autonomous LLM agent loops, served over HTTP with local CLIs.

%pre
getent group wf >/dev/null || groupadd --system wf
getent passwd wf >/dev/null || useradd --system --gid wf \
    --home-dir /var/lib/wf --create-home --shell /usr/sbin/nologin wf
install -d -o wf -g wf -m 0750 /var/lib/wf /var/log/wf /etc/wf
exit 0

%post
# First install renders the managed config tree; upgrades never touch it.
if [ "$1" = "1" ]; then
    /usr/share/wf/deploy/scripts/render-config.sh \
        --templates /usr/share/wf/deploy/templates \
        --to managed --managed-dir /etc/wf >/dev/null 2>&1 || true
fi
%systemd_post wf-server.service

%preun
%systemd_preun wf-server.service

%postun
%systemd_postun wf-server.service
# Only purge data on full erase; plain remove keeps config and data.
if [ "$1" = "0" ]; then
    rm -rf /var/lib/wf /var/log/wf /etc/wf
fi

%files
%dir %attr(0750,wf,wf) /etc/wf
%dir %attr(0750,wf,wf) /var/lib/wf
%dir %attr(0750,wf,wf) /var/log/wf
%{_bindir}/wf-server
%{_bindir}/wf
%{_bindir}/wf-mini
%{_bindir}/wf-headless
%{_bindir}/wf-debug
/usr/share/wf/web
/usr/share/wf/deploy
%{_unitdir}/wf-server.service

%changelog
* Sat Nov 07 2026 wf-agent Contributors <wf-agent@example.com> - 0.1.0-1
- Ship wf-server plus the wf/wf-mini/wf-headless/wf-debug CLIs, the web
  static dir, deploy templates and the managed-config render script.
- Managed config is rendered on first install, never overwritten.
