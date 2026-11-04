@echo off
rem Runs Otto's MCP server from a copy of the built exe, so that a rebuild can always
rem relink target\debug\otto.exe while Claude keeps a server running: a server holds a
rem lock on the file it was started from, which used to make "cargo build" fail to link.
setlocal
set "ROOT=%~dp0.."
set "SRC=%ROOT%\target\debug\otto.exe"
set "DIR=%ROOT%\target\mcp"
rem A copy of its own per server, so a copy an earlier server still holds can never make
rem this one run a stale build. Copies of servers that have ended are cleared out here;
rem the ones still running simply stay (deleting them fails, which is fine).
if not exist "%DIR%" mkdir "%DIR%" >nul 2>&1
del /q "%DIR%\otto*.exe" >nul 2>&1
set "DST=%DIR%\otto-%RANDOM%%RANDOM%.exe"
copy /y "%SRC%" "%DST%" >nul 2>&1
if not exist "%DST%" (
  echo Otto MCP: no build to run. Build it first with "cargo build". 1>&2
  exit /b 1
)
rem stdin / stdout belong to the MCP protocol from here on.
"%DST%" mcp %*
