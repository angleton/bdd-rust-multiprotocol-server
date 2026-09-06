Feature: REST GET endpoint

  Scenario: Get a simple hello response
    Given the server is running
    When I send a REST GET request to "/hello"
    Then the response body should be "REST message"