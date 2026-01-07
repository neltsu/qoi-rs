if (-NOT(Test-Path 'RGFW.h')) {
    Invoke-WebRequest 'https://raw.githubusercontent.com/ColleagueRiley/RGFW/refs/heads/main/RGFW.h' -OutFile 'RGFW.h'
}

cargo build

cl.exe /nologo main.c /link ../target/debug/qoi_rs.dll.lib
