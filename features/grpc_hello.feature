Feature: gRPC hello service

  Scenario: Call the hello RPC
    Given the server is running
    When I call the gRPC hello method with the name "BDD"
    Then the gRPC response message should start with "gRPC in "
