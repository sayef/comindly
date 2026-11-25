package com.northvile.common;

import java.time.Clock;
import java.time.Instant;
import java.time.LocalDate;
import java.time.ZoneOffset;

/** The only clock services use; trial data is stored and compared in UTC. */
public final class UtcClock {
    private static Clock clock = Clock.systemUTC();

    private UtcClock() {
    }

    public static Instant now() {
        return clock.instant();
    }

    public static LocalDate today() {
        return LocalDate.ofInstant(clock.instant(), ZoneOffset.UTC);
    }

    static void fixAt(Instant instant) {
        clock = Clock.fixed(instant, ZoneOffset.UTC);
    }
}
