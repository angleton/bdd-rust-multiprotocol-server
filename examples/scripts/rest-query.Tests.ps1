<#
    This is the first PowerShell-shaped version of the idea behind this project.

    Before the Rust server existed, the useful question was much smaller:

        "Can I describe a request, send it, and prove what came back?"

    Pester gives PowerShell a BDD-like vocabulary for that experiment:

        Describe = the capability under test
        Context  = the situation or setup
        It       = the behavior we expect

    The script intentionally stays simple. It does not start the Rust server;
    start it in another terminal with `cargo run`, then run this file with
    `Invoke-Pester`. That separation makes the first experiment easy to see
    and leaves server startup to the application being tested.

    Example:

        cargo run
        Invoke-Pester .\examples\scripts\rest-query.Tests.ps1 -PassThru

    A different server address or payload can be supplied when debugging:

        $env:REST_BASE_URL = "http://127.0.0.1:8080"
        $env:REST_PAYLOAD = "debug"
        Invoke-Pester .\examples\scripts\rest-query.Tests.ps1
#>

# The installed Pester 3/4-style command does not provide the newer
# -Parameters option. Environment variables keep the example configurable
# without hiding the basic request in a helper module.
$BaseUrl = if ($env:REST_BASE_URL) { $env:REST_BASE_URL } else { "http://127.0.0.1:8080" }
$Payload = if ($env:REST_PAYLOAD) { $env:REST_PAYLOAD } else { "first-rest-test" }

Describe "REST hello endpoint" {
    # Given: the Rust server is running at the configured address.
    BeforeAll {
        $healthUri = "$BaseUrl/health"

        try {
            $healthResponse = Invoke-RestMethod -Uri $healthUri -Method Get -ErrorAction Stop
        }
        catch {
            throw "Given the server is running failed. Start it with 'cargo run' and retry. URI: $healthUri. Error: $($_.Exception.Message)"
        }

        $healthResponse | Should Be "ok"
    }

    Context "When I send a REST GET request with a payload" {
        BeforeAll {
            # When: construct the same request shape documented by the Rust
            # feature and benchmark tests.
            $encodedPayload = [System.Uri]::EscapeDataString($Payload)
            $requestUri = "$BaseUrl/hello?payload=$encodedPayload"

            try {
                $response = Invoke-RestMethod -Uri $requestUri -Method Get -ErrorAction Stop
            }
            catch {
                throw "When the REST request was sent, the call failed. URI: $requestUri. Error: $($_.Exception.Message)"
            }

            # Keep the response available to each It block. This mirrors a BDD
            # scenario's shared world without introducing another framework.
            $script:RestResponse = $response
            $script:RestRequestUri = $requestUri
        }

        It "Then the response identifies the REST protocol" {
            $script:RestResponse | Should Be "REST message"
        }

        It "And the request URL contains the payload used by the scenario" {
            $script:RestRequestUri | Should Match "[?&]payload="
        }
    }
}
