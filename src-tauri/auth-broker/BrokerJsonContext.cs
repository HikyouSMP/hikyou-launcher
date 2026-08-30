using System.Text.Json.Serialization;

[JsonSourceGenerationOptions(PropertyNamingPolicy = JsonKnownNamingPolicy.SnakeCaseLower)]
[JsonSerializable(typeof(BrokerResponse))]
[JsonSerializable(typeof(BrokerError))]
internal partial class BrokerJsonContext : JsonSerializerContext;
