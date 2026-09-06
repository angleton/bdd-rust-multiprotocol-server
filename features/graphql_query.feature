Feature: GraphQL query endpoint

  Scenario: Query a simple hello response
    Given the server is running
    When I send a GraphQL query for "hello"
    Then the GraphQL response data should identify "GraphQL"
