import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import net.fabricmc.loader.api.Version;
import net.fabricmc.loader.api.metadata.version.VersionPredicate;

// Test-only reference process. The class path contains only the verified Loader JAR.
public class FabricPredicateOracle {
    public static void main(String[] args) throws Exception {
        try (BufferedReader input = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = input.readLine()) != null) {
                String[] fields = line.split("\t", -1);
                if (fields.length != 2) throw new IllegalArgumentException("Expected two TSV fields");
                System.out.println(VersionPredicate.parse(fields[1]).test(Version.parse(fields[0])));
            }
        }
    }
}
