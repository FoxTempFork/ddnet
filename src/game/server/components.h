// This file can be included several times.

#ifndef COMPONENT
#error "The component macros must be defined"
#define COMPONENT(Member, Type, Getter, ...) ;
#endif

COMPONENT(m_pTeeHistorian, CTeeHistorianComponent, TeeHistorian)

/*
 * Add components for mods below this comment to avoid merge conflicts.
 */
