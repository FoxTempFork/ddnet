#include "teehistorian.h"

#include <base/aio.h>
#include <base/fs.h>
#include <base/io.h>
#include <base/logger.h>
#include <base/mem.h>
#include <base/str.h>

#include <engine/map.h>
#include <engine/server.h>
#include <engine/shared/config.h>
#include <engine/shared/uuid_manager.h>
#include <engine/storage.h>

#include <game/prng.h>
#include <game/server/entities/character.h>
#include <game/server/gamecontext.h>
#include <game/server/gamecontroller.h>
#include <game/server/player.h>
#include <game/version.h>

#include <ctime>

CTeeHistorianComponent::CTeeHistorianComponent()
{
	m_pFile = nullptr;
	m_Active = false;
}

void CTeeHistorianComponent::OnInit(const void *pPersistentData)
{
	m_Active = Config()->m_SvTeeHistorian;
	if(!m_Active)
	{
		return;
	}

	char aGameUuid[UUID_MAXSTRSIZE];
	FormatUuid(GameServer()->GameUuid(), aGameUuid, sizeof(aGameUuid));

	char aFilename[IO_MAX_PATH_LENGTH];
	str_format(aFilename, sizeof(aFilename), "teehistorian/%s.teehistorian", aGameUuid);

	IOHANDLE THFile = Storage()->OpenFile(aFilename, IOFLAG_WRITE, IStorage::TYPE_SAVE);
	if(!THFile)
	{
		dbg_msg("teehistorian", "failed to open '%s'", aFilename);
		Server()->SetErrorShutdown("teehistorian open error");
		m_Active = false;
		return;
	}
	else
	{
		dbg_msg("teehistorian", "recording to '%s'", aFilename);
	}
	m_pFile = aio_new(THFile);

	char aVersion[128];
	if(GIT_SHORTREV_HASH)
	{
		str_format(aVersion, sizeof(aVersion), "%s (%s)", GAME_VERSION, GIT_SHORTREV_HASH);
	}
	else
	{
		str_copy(aVersion, GAME_VERSION);
	}

	const CGameContext::CPersistentData *pPersistent = (const CGameContext::CPersistentData *)pPersistentData;

	CTeeHistorian::CGameInfo GameInfo;
	GameInfo.m_GameUuid = GameServer()->GameUuid();
	GameInfo.m_pServerVersion = aVersion;
	GameInfo.m_StartTime = time(nullptr);
	GameInfo.m_pPrngDescription = GameServer()->Prng()->Description();

	GameInfo.m_pServerName = Config()->m_SvName;
	GameInfo.m_ServerPort = Server()->Port();
	GameInfo.m_pGameType = GameServer()->Controller()->m_pGameType;

	GameInfo.m_pConfig = Config();
	GameInfo.m_pTuning = GameServer()->GlobalTuning();
	GameInfo.m_pUuids = &g_UuidManager;

	GameInfo.m_pMapName = GameServer()->Map()->BaseName();
	GameInfo.m_MapSize = GameServer()->Map()->Size();
	GameInfo.m_MapSha256 = GameServer()->Map()->Sha256();
	GameInfo.m_MapCrc = GameServer()->Map()->Crc();

	if(pPersistent)
	{
		GameInfo.m_HavePrevGameUuid = true;
		GameInfo.m_PrevGameUuid = pPersistent->m_PrevGameUuid;
	}
	else
	{
		GameInfo.m_HavePrevGameUuid = false;
		mem_zero(&GameInfo.m_PrevGameUuid, sizeof(GameInfo.m_PrevGameUuid));
	}

	m_TeeHistorian.Reset(&GameInfo, TeeHistorianWrite, this);
}

void CTeeHistorianComponent::OnConsoleInit(IConsole *pConsole)
{
	pConsole->SetTeeHistorianCommandCallback(CommandCallback, this);
}

bool CTeeHistorianComponent::IsEnabled()
{
	return CServerComponent::IsEnabled() && m_Active;
}

void CTeeHistorianComponent::OnShutdown(void *pPersistentData)
{
	if(!m_Active)
	{
		return;
	}
	m_TeeHistorian.Finish();
	aio_close(m_pFile);
	aio_wait(m_pFile);
	int Error = aio_error(m_pFile);
	if(Error)
	{
		dbg_msg("teehistorian", "error closing file, err=%d", Error);
		Server()->SetErrorShutdown("teehistorian close error");
	}
	aio_free(m_pFile);
	m_pFile = nullptr;
	m_Active = false;
}

void CTeeHistorianComponent::OnPreTick()
{
	if(!m_Active)
	{
		return;
	}

	CGameContext *pGameServer = GameServer();
	for(int i = 0; i < MAX_CLIENTS; i++)
	{
		if(pGameServer->m_apPlayers[i] != nullptr)
		{
			m_TeeHistorian.RecordPlayerTeam(i, pGameServer->GetDDRaceTeam(i));
		}
		else
		{
			m_TeeHistorian.RecordPlayerTeam(i, 0);
		}
	}
	for(int i = 0; i < TEAM_SUPER; i++)
	{
		m_TeeHistorian.RecordTeamPractice(i, pGameServer->Controller()->Teams().IsPractice(i));
	}
}

void CTeeHistorianComponent::OnTickBegin(int Tick)
{
	if(!m_Active)
	{
		return;
	}

	int Error = aio_error(m_pFile);
	if(Error)
	{
		dbg_msg("teehistorian", "error writing to file, err=%d", Error);
		Server()->SetErrorShutdown("teehistorian io error");
	}

	if(!m_TeeHistorian.Starting())
	{
		m_TeeHistorian.EndInputs();
		m_TeeHistorian.EndTick();
	}
	m_TeeHistorian.BeginTick(Tick);
	m_TeeHistorian.BeginPlayers();
}

void CTeeHistorianComponent::OnTickPlayersEnd()
{
	if(!m_Active)
	{
		return;
	}

	CGameContext *pGameServer = GameServer();
	for(int i = 0; i < MAX_CLIENTS; i++)
	{
		CPlayer *pPlayer = pGameServer->m_apPlayers[i];
		if(pPlayer && pPlayer->GetCharacter())
		{
			CNetObj_CharacterCore Char;
			pPlayer->GetCharacter()->GetCore().Write(&Char);
			m_TeeHistorian.RecordPlayer(i, &Char);
		}
		else
		{
			m_TeeHistorian.RecordDeadPlayer(i);
		}
	}
	m_TeeHistorian.EndPlayers();
	m_TeeHistorian.BeginInputs();
}

void CTeeHistorianComponent::RecordConsoleCommand(int ClientId, int FlagMask, const char *pCmd, IConsole::IResult *pResult)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordConsoleCommand(ClientId, FlagMask, pCmd, pResult);
	}
}

void CTeeHistorianComponent::RecordPlayerTeam(int ClientId, int Team)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerTeam(ClientId, Team);
	}
}

void CTeeHistorianComponent::RecordTeamPractice(int Team, bool Practice)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamPractice(Team, Practice);
	}
}

void CTeeHistorianComponent::RecordPlayerInput(int ClientId, uint32_t UniqueClientId, const CNetObj_PlayerInput *pInput)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerInput(ClientId, UniqueClientId, pInput);
	}
}

void CTeeHistorianComponent::RecordPlayerReady(int ClientId)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerReady(ClientId);
	}
}

void CTeeHistorianComponent::RecordAntibot(const void *pData, int DataSize)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordAntibot(pData, DataSize);
	}
}

void CTeeHistorianComponent::RecordPlayerJoin(int ClientId, int Protocol)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerJoin(ClientId, Protocol);
	}
}

void CTeeHistorianComponent::RecordPlayerDrop(int ClientId, const char *pReason)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerDrop(ClientId, pReason);
	}
}

void CTeeHistorianComponent::RecordPlayerRejoin(int ClientId)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerRejoin(ClientId);
	}
}

void CTeeHistorianComponent::RecordPlayerName(int ClientId, const char *pName)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerName(ClientId, pName);
	}
}

void CTeeHistorianComponent::RecordPlayerFinish(int ClientId, int TimeTicks)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerFinish(ClientId, TimeTicks);
	}
}

void CTeeHistorianComponent::RecordTeamFinish(int TeamId, int TimeTicks)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamFinish(TeamId, TimeTicks);
	}
}

void CTeeHistorianComponent::RecordAuthLogin(int ClientId, int Level, const char *pAuthName)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordAuthLogin(ClientId, Level, pAuthName);
	}
}

void CTeeHistorianComponent::RecordAuthLogout(int ClientId)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordAuthLogout(ClientId);
	}
}

void CTeeHistorianComponent::RecordDDNetVersion(int ClientId, CUuid ConnectionId, int DDNetVersion, const char *pDDNetVersionStr)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordDDNetVersion(ClientId, ConnectionId, DDNetVersion, pDDNetVersionStr);
	}
}

void CTeeHistorianComponent::RecordDDNetVersionOld(int ClientId, int DDNetVersion)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordDDNetVersionOld(ClientId, DDNetVersion);
	}
}

void CTeeHistorianComponent::RecordPlayerMessage(int ClientId, const void *pMsg, int MsgSize)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerMessage(ClientId, pMsg, MsgSize);
	}
}

void CTeeHistorianComponent::RecordPlayerSwap(int ClientId1, int ClientId2)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordPlayerSwap(ClientId1, ClientId2);
	}
}

void CTeeHistorianComponent::RecordTeamSaveSuccess(int Team, CUuid SaveId, const char *pTeamSave)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamSaveSuccess(Team, SaveId, pTeamSave);
	}
}

void CTeeHistorianComponent::RecordTeamSaveFailure(int Team)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamSaveFailure(Team);
	}
}

void CTeeHistorianComponent::RecordTeamLoadSuccess(int Team, CUuid SaveId, const char *pTeamSave)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamLoadSuccess(Team, SaveId, pTeamSave);
	}
}

void CTeeHistorianComponent::RecordTeamLoadFailure(int Team)
{
	if(m_Active)
	{
		m_TeeHistorian.RecordTeamLoadFailure(Team);
	}
}

void CTeeHistorianComponent::TeeHistorianWrite(const void *pData, int DataSize, void *pUser)
{
	CTeeHistorianComponent *pSelf = (CTeeHistorianComponent *)pUser;
	aio_write(pSelf->m_pFile, pData, DataSize);
}

void CTeeHistorianComponent::CommandCallback(int ClientId, int FlagMask, const char *pCmd, IConsole::IResult *pResult, void *pUser)
{
	CTeeHistorianComponent *pSelf = (CTeeHistorianComponent *)pUser;
	pSelf->RecordConsoleCommand(ClientId, FlagMask, pCmd, pResult);
}
