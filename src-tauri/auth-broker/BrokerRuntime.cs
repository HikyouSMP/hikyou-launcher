using System.Runtime.InteropServices;

static class BrokerRuntime
{
    private const uint SearchDllDirectory = 0x00000100;
    private const uint SearchSystem32 = 0x00000800;

    public static void Load()
    {
        var filename = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "msalruntime.dll",
            Architecture.Arm64 => "msalruntime_arm64.dll",
            _ => throw new BrokerFailure("unsupported_architecture", "Unsupported authentication runtime architecture."),
        };
        var path = Path.Combine(AppContext.BaseDirectory, "auth-runtime", filename);
        if (!File.Exists(path))
            throw new BrokerFailure("runtime_missing", "The authentication runtime is missing. Please reinstall Hikyou Launcher.");

        // Keep the module loaded for MSAL's lifetime; never search PATH or the working directory.
        var module = LoadLibraryExW(path, 0, SearchDllDirectory | SearchSystem32);
        if (module == 0)
            throw new BrokerFailure("runtime_load_failed", $"Authentication runtime could not load (Windows error {Marshal.GetLastWin32Error()}).");
        if (!NativeLibrary.TryGetExport(module, "MSALRUNTIME_Startup", out _))
            throw new BrokerFailure("runtime_incompatible", "The authentication runtime is incompatible. Please reinstall Hikyou Launcher.");
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    private static extern nint LoadLibraryExW(string filename, nint file, uint flags);
}
