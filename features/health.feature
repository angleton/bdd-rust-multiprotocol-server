Feature: Health check

  Scenario: Server returns ok from health endpoint
    Given the server is running
    When I request the health endpoint
    Then the response should be ok
