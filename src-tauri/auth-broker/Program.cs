using System.Text.Json;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Identity.Client;
using Microsoft.Identity.Client.Broker;

const string ClientId = GeneratedAuthConfig.ClientId;
string[] scopes = ["XboxLive.SignIn", "XboxLive.offline_access"];

try
{
    BrokerRuntime.Load();
    if (args is ["--self-test"])
    {
        Console.Out.Write("{\"status\":\"ok\"}");
        return;
    }
    if (string.IsNullOrWhiteSpace(ClientId))
    {
        throw new BrokerFailure(
            "configuration_error",
            "Microsoft authentication is not configured in this build.");
    }
    var request = BrokerRequest.Parse(args);
    var options = new BrokerOptions(BrokerOptions.OperatingSystems.Windows)
    {
        Title = "Hikyou Launcher",
        ListOperatingSystemAccounts = true,
    };
    var app = PublicClientApplicationBuilder
        .Create(ClientId)
        .WithAuthority(AadAuthorityAudience.PersonalMicrosoftAccount)
        .WithDefaultRedirectUri()
        .WithParentActivityOrWindow(() => request.ParentWindow != 0
            ? request.ParentWindow
            : NativeMethods.GetDesktopWindow())
        .WithBroker(options)
        .Build();

    AuthenticationResult result;
    if (request.Interactive)
    {
        result = await app
            .AcquireTokenInteractive(scopes)
            .WithPrompt(Prompt.SelectAccount)
            .ExecuteAsync();
    }
    else
    {
        var accounts = await app.GetAccountsAsync();
        var account = request.AccountKey is null
            ? accounts.SingleOrDefault()
            : accounts.FirstOrDefault(candidate =>
                StringComparer.Ordinal.Equals(AccountKey(candidate), request.AccountKey));
        if (account is null)
        {
            throw new BrokerFailure("interaction_required", "No matching WAM account is available.");
        }
        result = await app.AcquireTokenSilent(scopes, account).ExecuteAsync();
    }

    var response = new BrokerResponse(
        result.AccessToken,
        result.ExpiresOn.ToUnixTimeSeconds(),
        AccountKey(result.Account));
    Console.Out.Write(JsonSerializer.Serialize(response, BrokerJsonContext.Default.BrokerResponse));
}
catch (MsalUiRequiredException)
{
    Fail("interaction_required", "WAM requires interactive authentication.");
}
catch (MsalClientException error) when (error.ErrorCode == "authentication_canceled")
{
    Fail("cancelled", "Authentication was cancelled.");
}
catch (BrokerFailure error)
{
    Fail(error.Code, error.Message);
}
catch (MsalException error)
{
    Fail("msal_error", SafeCode(error.ErrorCode));
}
catch (Exception error)
{
    Fail("broker_exception", SafeCode(error.GetType().Name));
}

static string SafeCode(string value) => value.Length is > 0 and <= 80
    && value.All(character => char.IsAsciiLetterOrDigit(character) || character == '_')
        ? value : "unknown";

static void Fail(string code, string message)
{
    var error = new BrokerError(code, message);
    Console.Out.Write(JsonSerializer.Serialize(error, BrokerJsonContext.Default.BrokerError));
    Environment.ExitCode = code == "cancelled" ? 2 : 3;
}

static string AccountKey(IAccount account) => Convert.ToHexString(
    SHA256.HashData(Encoding.UTF8.GetBytes(account.HomeAccountId.Identifier)))
    .ToLowerInvariant();

sealed record BrokerRequest(bool Interactive, string? AccountKey, nint ParentWindow)
{
    public static BrokerRequest Parse(string[] args)
    {
        var interactive = false;
        string? accountKey = null;
        nint parentWindow = 0;
        for (var index = 0; index < args.Length; index++)
        {
            switch (args[index])
            {
                case "--interactive":
                    interactive = true;
                    break;
                case "--silent":
                    break;
                case "--account-key" when index + 1 < args.Length:
                    accountKey = args[++index];
                    break;
                case "--parent-window" when index + 1 < args.Length:
                    if (!long.TryParse(args[++index], out var rawWindow))
                    {
                        throw new BrokerFailure("invalid_request", "The parent window handle is invalid.");
                    }
                    parentWindow = (nint)rawWindow;
                    break;
                default:
                    throw new BrokerFailure("invalid_request", "The broker arguments are invalid.");
            }
        }
        return new BrokerRequest(interactive, accountKey, parentWindow);
    }
}

sealed record BrokerResponse(string AccessToken, long ExpiresAt, string AccountKey);
sealed record BrokerError(string Error, string Message);

sealed class BrokerFailure(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}

static class NativeMethods
{
    [DllImport("user32.dll")]
    internal static extern nint GetDesktopWindow();
}
