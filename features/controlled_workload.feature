Feature: Controlled workload for protocol comparisons

  Scenario: Apply the same requested workload through every protocol
    Given the server is running
    And I request a target CPU load of 3 percent for 100 milliseconds
    When I send the controlled workload through every protocol
    Then every protocol response should succeed
    And telemetry should report the requested 3 percent load for 100 milliseconds
