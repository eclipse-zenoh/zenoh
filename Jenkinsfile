pipeline {
  agent { label 'UbuntuVM' }
  parameters {
    gitParameter(name: 'GIT_TAG',
                 type: 'PT_TAG',
                 description: 'The Git tag to checkout. If not specified "master" will be checkout.',
                 defaultValue: 'master')
    string(name: 'DOCKER_TAG',
           description: 'An extra Docker tag (e.g. "latest"). By default GIT_TAG will also be used as Docker tag')
  }

  stages {
  // Same build (without Docker) on MacMini
    stage('[MacMini] Checkout Git TAG') {
      agent { label 'MacMini' }
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.GIT_TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/eclipse-zenoh/zenoh.git']]
                ])
      }
    }
    stage('[MacMini] Setup opam dependencies') {
      agent { label 'MacMini' }
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        OPAMJOBS=1 opam config report
        OPAMJOBS=1 opam install depext conf-libev
        OPAMJOBS=1 opam depext -yt
        OPAMJOBS=1 opam install -t . --deps-only
        '''
      }
    }
    stage('[MacMini] Build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        opam exec -- dune build @all
        '''
      }
    }
    stage('[MacMini] Tests') {
      agent { label 'MacMini' }
      steps {
        sh '''
        opam exec -- dune runtest
        '''
      }
    }
    stage('[MacMini] Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        cp -r _build/default/install eclipse-zenoh
        tar czvf eclipse-zenoh-${GIT_TAG}-macosx-x86-64.tgz eclipse-zenoh/*/*.*
        '''
        stash includes: 'eclipse-zenoh-*-macosx-x86-64.tgz', name: 'zenohMacOS'
      }
    }

    stage('[Ubuntu] Checkout Git TAG') {
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.GIT_TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/eclipse-zenoh/zenoh.git']]
                ])
      }
    }
    stage('[Ubuntu] Setup opam dependencies') {
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        OPAMJOBS=1 opam config report
        OPAMJOBS=1 opam install depext conf-libev
        OPAMJOBS=1 opam depext -yt
        OPAMJOBS=1 opam install -t . --deps-only
        '''
      }
    }
    stage('[Ubuntu] Build') {
      steps {
        sh '''
        opam exec -- dune build @all
        '''
      }
    }
    stage('[Ubuntu] Tests') {
      steps {
        sh '''
        opam exec -- dune runtest
        '''
      }
    }
    stage('[Ubuntu] Package') {
      steps {
        sh '''
        cp -r _build/default/install eclipse-zenoh
        tar czvf eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz eclipse-zenoh/*/*.*
        '''
      }
    }

    stage('[Ubuntu] Docker build') {
      steps {
        sh '''
        if [ -n "${DOCKER_TAG}" ]; then
          export EXTRA_TAG="-t eclipse/zenoh:${DOCKER_TAG}"
        fi
        docker build -t eclipse/zenoh:${GIT_TAG} ${EXTRA_TAG} .
        '''
      }
    }
    stage('[Ubuntu] Docker publish') {
      steps {
        withCredentials([usernamePassword(credentialsId: 'dockerhub-bot',
            passwordVariable: 'DOCKER_HUB_CREDS_PSW', usernameVariable: 'DOCKER_HUB_CREDS_USR')])
        {
          sh '''
          docker login -u ${DOCKER_HUB_CREDS_USR} -p ${DOCKER_HUB_CREDS_PSW}
          docker push eclipse/zenoh
          docker logout
          '''
        }
      }
    }

    stage('Deploy to to download.eclipse.org') {
      steps {
        // Unstash MacOS package to be deployed
        unstash 'zenohMacOS'
        sshagent ( ['projects-storage.eclipse.org-bot-ssh']) {
          sh '''
          ssh genie.zenoh@projects-storage.eclipse.org mkdir -p /home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}
          ssh genie.zenoh@projects-storage.eclipse.org ls -al /home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}
          scp eclipse-zenoh-${GIT_TAG}-macosx-x86-64.tgz  eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz  genie.zenoh@projects-storage.eclipse.org:/home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}/
          '''
        }
      }
    }

  }

  post {
    success {
        archiveArtifacts artifacts: 'eclipse-zenoh-${GIT_TAG}-macosx-x86-64.tgz, eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz', fingerprint: true
    }
  }
}
