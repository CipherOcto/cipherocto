# CipherOcto Organization Chart

> **Note:** This document represents the planned organizational structure. Actual team composition will evolve as the project grows.

---

## Executive Leadership

```mermaid
graph TB
    CEO[CEO<br/>Chief Executive Officer]
    CTO[CTO<br/>Chief Technology Officer]
    CFO[CFO<br/>Chief Financial Officer]
    COO[COO<br/>Chief Operating Officer]

    CEO --> CTO
    CEO --> CFO
    CEO --> COO

    style CEO fill:#9b59b6
    style CTO fill:#3498db
    style CFO fill:#e74c3c
    style COO fill:#27ae60
```

---

## Engineering Department

**Head:** CTO

```mermaid
graph TB
    CTO[CTO]
    HeadProtocol[Head of Protocol]
    HeadInfra[Head of Infrastructure]
    HeadSecurity[Head of Security]

    CTO --> HeadProtocol
    CTO --> HeadInfra
    CTO --> HeadSecurity

    subgraph PROTOCOL["Protocol Team"]
        ProtocolTeam1[Smart Contracts]
        ProtocolTeam2[Consensus]
        ProtocolTeam3[Cryptography]
    end

    subgraph INFRA["Infrastructure Team"]
        InfraTeam1[Node Software]
        InfraTeam2[DevOps]
        InfraTeam3[Data Engineering]
    end

    subgraph SECURITY["Security Team"]
        SecurityTeam1[Audits]
        SecurityTeam2[Red Team]
        SecurityTeam3[Compliance]
    end

    HeadProtocol --> ProtocolTeam1
    HeadProtocol --> ProtocolTeam2
    HeadProtocol --> ProtocolTeam3

    HeadInfra --> InfraTeam1
    HeadInfra --> InfraTeam2
    HeadInfra --> InfraTeam3

    HeadSecurity --> SecurityTeam1
    HeadSecurity --> SecurityTeam2
    HeadSecurity --> SecurityTeam3

    style CTO fill:#3498db
    style HeadProtocol fill:#9b59b6
    style HeadInfra fill:#27ae60
    style HeadSecurity fill:#e74c3c
```

---

## Product Department

**Head:** Chief Product Officer (reports to CEO)

```mermaid
graph TB
    CPO[Chief Product Officer]
    HeadDesign[Head of Design]
    HeadPM[Head of Product Management]
    HeadData[Head of Data Science]

    CPO --> HeadDesign
    CPO --> HeadPM
    CPO --> HeadData

    subgraph DESIGN["Design Team"]
        DesignTeam1[UX Research]
        DesignTeam2[UI Design]
        DesignTeam3[Brand]
    end

    subgraph PM["Product Management"]
        PMTeam1[Core Product]
        PMTeam2[Developer Experience]
        PMTeam3[Enterprise]
    end

    subgraph DATA["Data Science"]
        DataTeam1[ML Research]
        DataTeam2[Analytics]
        DataTeam3[Tokenomics]
    end

    HeadDesign --> DesignTeam1
    HeadDesign --> DesignTeam2
    HeadDesign --> DesignTeam3

    HeadPM --> PMTeam1
    HeadPM --> PMTeam2
    HeadPM --> PMTeam3

    HeadData --> DataTeam1
    HeadData --> DataTeam2
    HeadData --> DataTeam3

    style CPO fill:#f39c12
    style HeadDesign fill:#9b59b6
    style HeadPM fill:#3498db
    style HeadData fill:#27ae60
```

---

## Growth Department

**Head:** Chief Growth Officer (reports to CEO)

```mermaid
graph TB
    CGO[Chief Growth Officer]
    HeadMarketing[Head of Marketing]
    HeadCommunity[Head of Community]
    HeadBD[Head of Business Development]
    HeadContent[Head of Content]

    CGO --> HeadMarketing
    CGO --> HeadCommunity
    CGO --> HeadBD
    CGO --> HeadContent

    subgraph MARKETING["Marketing Team"]
        MarketingTeam1[Growth Marketing]
        MarketingTeam2[Performance Marketing]
        MarketingTeam3[PR & Communications]
    end

    subgraph COMMUNITY["Community Team"]
        CommunityTeam1[Discord & Social]
        CommunityTeam2[Events]
        CommunityTeam3[Developer Relations]
    end

    subgraph BD["Business Development"]
        BDTeam1[Partnerships]
        BDTeam2[Enterprise Sales]
        BDTeam3[Exchanges & Listings]
    end

    subgraph CONTENT["Content Team"]
        ContentTeam1[Documentation]
        ContentTeam2[Education]
        ContentTeam3[Thought Leadership]
    end

    HeadMarketing --> MarketingTeam1
    HeadMarketing --> MarketingTeam2
    HeadMarketing --> MarketingTeam3

    HeadCommunity --> CommunityTeam1
    HeadCommunity --> CommunityTeam2
    HeadCommunity --> CommunityTeam3

    HeadBD --> BDTeam1
    HeadBD --> BDTeam2
    HeadBD --> BDTeam3

    HeadContent --> ContentTeam1
    HeadContent --> ContentTeam2
    HeadContent --> ContentTeam3

    style CGO fill:#e74c3c
    style HeadMarketing fill:#9b59b6
    style HeadCommunity fill:#3498db
    style HeadBD fill:#27ae60
    style HeadContent fill:#f39c12
```

---

## Operations Department

**Head:** COO

```mermaid
graph TB
    COO[COO]
    HeadOps[Head of Operations]
    HeadFinance[Head of Finance]
    HeadLegal[Head of Legal]
    HeadPeople[Head of People]

    COO --> HeadOps
    COO --> HeadFinance
    COO --> HeadLegal
    COO --> HeadPeople

    subgraph OPS["Operations Team"]
        OpsTeam1[Vendor Management]
        OpsTeam2[Office Management]
        OpsTeam3[IT Support]
    end

    subgraph FINANCE["Finance Team"]
        FinanceTeam1[Accounting]
        FinanceTeam2[Treasury]
        FinanceTeam3[FP&A]
    end

    subgraph LEGAL["Legal Team"]
        LegalTeam1[Corporate]
        LegalTeam2[Regulatory]
        LegalTeam3[IP]
    end

    subgraph PEOPLE["People Team"]
        PeopleTeam1[Recruiting]
        PeopleTeam2[HR Operations]
        PeopleTeam3[Culture]
    end

    HeadOps --> OpsTeam1
    HeadOps --> OpsTeam2
    HeadOps --> OpsTeam3

    HeadFinance --> FinanceTeam1
    HeadFinance --> FinanceTeam2
    HeadFinance --> FinanceTeam3

    HeadLegal --> LegalTeam1
    HeadLegal --> LegalTeam2
    HeadLegal --> LegalTeam3

    HeadPeople --> PeopleTeam1
    HeadPeople --> PeopleTeam2
    HeadPeople --> PeopleTeam3

    style COO fill:#27ae60
    style HeadOps fill:#9b59b6
    style HeadFinance fill:#3498db
    style HeadLegal fill:#e74c3c
    style HeadPeople fill:#f39c12
```

---

## Hiring Roadmap

### Phase 1: Foundation Team (2026)

| Role | Status | Priority |
| ---- | ------ | -------- |
| CEO | 🔄 Hiring | Critical |
| CTO | 🔄 Hiring | Critical |
| Smart Contract Engineer | 📅 Q2 2026 | High |
| Protocol Engineer | 📅 Q2 2026 | High |
| DevOps Engineer | 📅 Q3 2026 | Medium |

### Phase 2: Growth Team (2027)

| Role | Target | Priority |
| ---- | ------ | -------- |
| CFO | Q1 2027 | Critical |
| Head of Product | Q1 2027 | High |
| Head of Community | Q2 2027 | High |
| Security Researcher | Q2 2027 | High |
| 3 Protocol Engineers | Q2-Q4 2027 | High |

### Phase 3: Scale Team (2028+)

| Role | Target | Headcount |
| ---- | ------ | --------- |
| Protocol engineers | 2028-2029 | 10 |
| Full-stack developers | 2028-2029 | 8 |
| DevOps | 2028-2029 | 5 |
| Security | 2028-2029 | 3 |
| Product | 2028-2029 | 5 |
| Marketing | 2028-2029 | 8 |
| Community | 2028-2029 | 5 |
| Operations | 2028-2029 | 5 |
| **Total** | | ~50 FTE |

---

## Open Positions

**Current openings:** See [careers.cipherocto.io](https://careers.cipherocto.io)

**Interested in joining?** Email your resume to: careers@cipherocto.io

---

*Note: This org chart is a planning document. Actual structure will evolve based on needs and candidates.*
